use byteorder::{ReadBytesExt, LE};
use chrono::prelude::*;
use clap::{App, Arg};
use failure_derive::Fail;
use lazy_static::lazy_static;
use rayon::prelude::*;
use std::ffi::OsStr;
use std::fs;
use std::io::prelude::*;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Duration;

const EXT: &str = "ioym";

lazy_static! {
    static ref OFFSET: chrono::FixedOffset = Local::now().offset().fix();
}

#[derive(Fail, Debug)]
enum Error {
    #[fail(display = "Unsupported file type for \"{}\"", _0)]
    UnsupportedFileType(String),

    #[fail(display = "File \"{}\" not found", _0)]
    FileNotFound(String),

    #[fail(display = "Outputting to standard output is not supported with multiple inputs")]
    StdoutForbidsMultipleInputs,
}

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
    fn with_reader(r: R) -> Result<Self, failure::Error> {
        Ok(Self {
            input: BufReader::new(zstd::Decoder::new(r)?),
            offset: None,
        })
    }
}

#[cfg(test)]
impl<R: BufRead> Ioym<R> {
    fn with_buf_reader(r: R) -> Result<Self, failure::Error> {
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

    fn decode<W: Write>(&mut self, output: &mut W) -> Result<(), failure::Error> {
        let mut output = std::io::BufWriter::with_capacity(
            zstd::Decoder::<BufReader<fs::File>>::recommended_output_size(),
            output,
        );

        loop {
            let ts = match read_time(&mut self.input, self.offset.unwrap_or(*OFFSET)) {
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => Err(e)?,
                Ok(ts) => ts,
            };
            write!(
                &mut output,
                "{}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} ",
                ts.year(),
                ts.month(),
                ts.day(),
                ts.hour(),
                ts.minute(),
                ts.second(),
                ts.nanosecond() / 1_000_000,
            )?;

            copy_until(&mut self.input, &mut output, b'\n')?;
        }

        Ok(())
    }
}

/// Read all bytes into `w` until the delimiter `byte` or EOF is reached.
///
/// Copied from [`BufRead::read_until`] and modified to write to `Write` instead of `Vec<u8>`.
fn copy_until<R, W>(r: &mut R, w: &mut W, delim: u8) -> Result<usize, failure::Error>
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

fn read_time<R: BufRead>(
    input: &mut R,
    offset: chrono::FixedOffset,
) -> Result<chrono::DateTime<FixedOffset>, std::io::Error> {
    let duration = Duration::from_millis(input.read_u64::<LE>()?);
    Ok(offset.timestamp(duration.as_secs() as i64, duration.subsec_nanos()))
}

fn open_ioym_file(filename: &str) -> Result<fs::File, failure::Error> {
    let path = Path::new(filename);

    if !path.exists() {
        Err(Error::FileNotFound(filename.to_string()))?;
    }

    match path.extension().and_then(OsStr::to_str) {
        Some(EXT) => (),
        _ => Err(Error::UnsupportedFileType(filename.to_string()))?,
    };

    Ok(fs::File::open(filename)?)
}

fn handle_file(filename: &str, output: Output, is_utc: bool) -> Result<(), failure::Error> {
    let mut ioym = Ioym::with_reader(open_ioym_file(filename)?)?;

    if is_utc {
        ioym.set_offset(Utc.fix());
    }

    match output {
        Output::Stdout => {
            let stdout = std::io::stdout();
            ioym.decode(&mut stdout.lock())?;
        }
        Output::File => {
            let output_file = &filename[..filename.len() - EXT.len() - 1];
            ioym.decode(&mut fs::OpenOptions::new().write(true).create_new(true).open(output_file)?)?;

            let metadata = fs::metadata(filename)?;
            filetime::set_file_times(
                output_file,
                filetime::FileTime::from_system_time(metadata.accessed().unwrap()),
                filetime::FileTime::from_system_time(metadata.modified().unwrap()),
            )?;

            fs::remove_file(filename)?;
        }
    }

    Ok(())
}

fn run() -> Result<(), failure::Error> {
    let matches = App::new("ioym")
        .version(option_env!("VERSION").unwrap_or("dev"))
        .about("Extracts and decodes host-io log files")
        .arg(
            Arg::with_name("stdout")
                .short("c")
                .long("stdout")
                .help("Output to standard output (only one file allowed)"),
        )
        .arg(
            Arg::with_name("utc")
                .short("u")
                .long("utc")
                .help("Use UTC instead of local timezone"),
        )
        .arg(Arg::with_name("file").required(true).multiple(true))
        .get_matches();

    let is_stdout = matches.is_present("stdout");
    let is_utc = matches.is_present("utc");
    let filenames = matches.values_of("file").unwrap();

    if is_stdout && filenames.len() > 1 {
        Err(Error::StdoutForbidsMultipleInputs)?;
    }

    filenames
        .collect::<Vec<_>>()
        .par_iter()
        .map(|filename| handle_file(filename, if is_stdout { Output::Stdout } else { Output::File }, is_utc))
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
