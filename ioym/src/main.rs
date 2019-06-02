use byteorder::{ReadBytesExt, LE};
use chrono::prelude::*;
use failure::Fail;
use lazy_static::lazy_static;
use rayon::prelude::*;
use std::ffi::OsStr;
use std::fs;
use std::io::prelude::*;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::time::Duration;
use structopt::StructOpt;

const EXT: &str = "ioym";

lazy_static! {
    static ref OFFSET: chrono::FixedOffset = Local::now().offset().fix();
}

#[derive(Fail, Debug)]
enum Error {
    #[fail(display = "I/O error: {}", _0)]
    Io(#[cause] io::Error),

    #[fail(display = "Unsupported file type for \"{}\"", _0)]
    UnsupportedFileType(String),

    #[fail(display = "Outputting to standard output is not supported with multiple inputs")]
    StdoutForbidsMultipleInputs,

    #[fail(display = "Line has invalid timestamp")]
    InvalidTimestamp,
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

type IoymResult<T> = Result<T, Error>;

#[derive(Clone, Copy)]
enum Output {
    Stdout,
    File,
}

struct Ioym<R: BufRead> {
    input: BufReader<zstd::Decoder<R>>,
    offset: Option<chrono::FixedOffset>,
}

impl<R: Read> Ioym<BufReader<R>> {
    fn with_reader(r: R) -> IoymResult<Self> {
        Ok(Self {
            input: BufReader::new(zstd::Decoder::new(r)?),
            offset: None,
        })
    }
}

#[cfg(test)]
impl<R: BufRead> Ioym<R> {
    fn with_buf_reader(r: R) -> IoymResult<Self> {
        Ok(Self {
            input: BufReader::new(zstd::Decoder::with_buffer(r)?),
            offset: None,
        })
    }
}

impl<R: BufRead> Ioym<R> {
    fn set_offset(&mut self, offset: chrono::FixedOffset) {
        self.offset = Some(offset);
    }

    fn decode<W: Write>(&mut self, output: &mut W) -> IoymResult<()> {
        let mut output = std::io::BufWriter::with_capacity(zstd::Decoder::<R>::recommended_output_size(), output);

        loop {
            match read_time(&mut self.input, self.offset.unwrap_or(*OFFSET)) {
                Err(Error::Io(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(Error::InvalidTimestamp) => (),
                Err(e) => Err(e)?,
                Ok(ts) => write!(
                    &mut output,
                    "{}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} ",
                    ts.year(),
                    ts.month(),
                    ts.day(),
                    ts.hour(),
                    ts.minute(),
                    ts.second(),
                    ts.nanosecond() / 1_000_000,
                )?,
            };

            copy_until(&mut self.input, &mut output, b'\n')?;
        }

        Ok(())
    }
}

/// Read all bytes into `w` until the delimiter `byte` or EOF is reached.
///
/// Copied from [`BufRead::read_until`] and modified to write to `Write` instead of `Vec<u8>`.
fn copy_until<R, W>(r: &mut R, w: &mut W, delim: u8) -> IoymResult<usize>
where
    R: BufRead,
    W: Write,
{
    let mut read = 0;
    loop {
        let (done, used) = {
            let available = match r.fill_buf() {
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => Err(e)?,
            };
            match memchr::memchr(delim, available) {
                Some(i) => {
                    w.write_all(&available[..i + 1])?;
                    (true, i + 1)
                }
                None => {
                    w.write_all(available)?;
                    (false, available.len())
                }
            }
        };
        r.consume(used);
        read += used;
        if done || used == 0 {
            return Ok(read);
        }
    }
}

fn read_time<R: BufRead>(input: &mut R, offset: chrono::FixedOffset) -> IoymResult<chrono::DateTime<FixedOffset>> {
    let duration = Duration::from_millis(input.read_u64::<LE>()?);
    match offset.timestamp_opt(duration.as_secs() as i64, duration.subsec_nanos()) {
        chrono::offset::LocalResult::Single(timestamp) => Ok(timestamp),
        _ => Err(Error::InvalidTimestamp),
    }
}

fn handle_file(filename: &Path, output: Output, is_utc: bool) -> IoymResult<()> {
    if filename.extension() != Some(OsStr::new(EXT)) {
        return Err(Error::UnsupportedFileType(filename.to_string_lossy().to_string()));
    }

    let mut ioym = Ioym::with_reader(fs::File::open(filename)?)?;

    if is_utc {
        ioym.set_offset(Utc.fix());
    }

    match output {
        Output::Stdout => {
            let stdout = std::io::stdout();
            ioym.decode(&mut stdout.lock())?;
        }
        Output::File => {
            let output_file = filename.parent().unwrap().join(filename.file_stem().unwrap());
            ioym.decode(&mut fs::OpenOptions::new().write(true).create_new(true).open(&output_file)?)?;

            let metadata = fs::metadata(filename)?;
            filetime::set_file_mtime(&output_file, metadata.modified()?.into())?;

            fs::remove_file(filename)?;
        }
    }

    Ok(())
}

#[derive(Debug, StructOpt)]
#[structopt(name = "ioym")]
/// Extracts and decodes loggest log files
struct Opt {
    #[structopt(long, short = "c")]
    /// Output to standard output (only one file allowed)
    stdout: bool,

    #[structopt(long, short)]
    /// Use UTC instead of local timezone
    utc: bool,

    #[structopt(parse(from_os_str), raw(required = "true"))]
    files: Vec<PathBuf>,
}

fn run() -> IoymResult<()> {
    let opt = Opt::from_args();

    if opt.stdout && opt.files.len() > 1 {
        return Err(Error::StdoutForbidsMultipleInputs);
    }

    opt.files
        .par_iter()
        .map(|filename| {
            handle_file(
                filename,
                if opt.stdout { Output::Stdout } else { Output::File },
                opt.utc,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod test {
    use chrono::{Offset, Utc};
    use std::io::Cursor;

    #[test]
    fn test_ioym_decode() {
        let compressed = include_bytes!("../samples/sample.ioym").to_vec();
        let sample_output = include_bytes!("../samples/sample").to_vec();

        let mut ioym = super::Ioym::with_buf_reader(Cursor::new(compressed)).unwrap();
        ioym.set_offset(Utc.fix());
        let mut output = Vec::new();
        ioym.decode(&mut output).unwrap();
        assert_eq!(output, sample_output);
    }
}
