//! `loggest` provides a high performance logging facility for Rust's [log](https://docs.rs/log) crate.
//!
//! Instead of writing logs to a file, `loggest` writes them to a pipe. The other end of the pipe is
//! opened by a daemon which is responsible for writing the logs (and possibly compressing them).

//! # Multithreading
//!
//! Each thread maintains its own pipe to avoid locking for each log line.

mod ignore;

use derive_more::From;
use failure::Fail;
use ignore::Ignore;
use log::{set_logger, set_max_level, LevelFilter, Log, Metadata, Record};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

static LOGGER: Loggest = Loggest;
static mut CONFIG: Option<Config> = None;
thread_local! {
    static OUTPUT: RefCell<Option<File>> = RefCell::new(None);
}

struct Loggest;

struct Config {
    level: LevelFilter,
    base_filename: PathBuf,
}

/// Error initializing `loggest`
#[derive(Debug, Fail, From)]
pub enum LoggestError {
    #[fail(display = "I/O error: {}", _0)]
    IoError(#[cause] io::Error),

    #[fail(display = "Set logger error: {}", _0)]
    SetLoggerError(#[cause] log::SetLoggerError),
}

/// Initialize `loggest`. Must only be called once.
///
/// The `base_filename` argument is used as the name for the main thread. Other threads append `.<thread_id>`.
///
/// # Example
/// ```no_run
/// loggest::init(log::LevelFilter::max(), "/var/log/my_app").unwrap();
/// ```
pub fn init<P: Into<PathBuf>>(level: LevelFilter, base_filename: P) -> Result<(), LoggestError> {
    set_logger(&LOGGER)?;
    set_max_level(level);
    let filename;
    unsafe {
        debug_assert!(CONFIG.is_none());
        CONFIG = Some(Config {
            level,
            base_filename: base_filename.into(),
        });
        filename = &CONFIG.as_ref().unwrap().base_filename;
    }

    let file = init_fifo(filename)?;
    OUTPUT.with(|output| {
        *output.borrow_mut() = Some(file);
    });
    Ok(())
}

fn init_fifo<P: AsRef<Path>>(filename: P) -> io::Result<File> {
    let filename = filename.as_ref();
    nix::unistd::mkfifo(filename, nix::sys::stat::Mode::from_bits(0o644).unwrap())
        .map_err(nix_error_to_io)?;
    OpenOptions::new().write(true).open(filename)
}

fn nix_error_to_io(err: nix::Error) -> io::Error {
    match err {
        nix::Error::Sys(errno) => errno.into(),
        _ => panic!(),
    }
}

impl Log for Loggest {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= unsafe { CONFIG.as_ref().unwrap().level }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        OUTPUT
            .with(|output| -> Result<(), Ignore> {
                let file = &mut *output.borrow_mut();
                if file.is_none() {
                    let filename = unsafe { &CONFIG.as_ref().unwrap().base_filename }
                        .join(format!(".{}", nix::unistd::gettid()));
                    *file = Some(init_fifo(filename)?);
                }
                let mut file = file.as_ref().unwrap();
                let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
                let now = now.as_secs() * 1000 + u64::from(now.subsec_millis());
                file.write_all(&now.to_le_bytes())?;
                write!(
                    file,
                    "[{}] {} -- {}\n",
                    record.level(),
                    record.target(),
                    record.args()
                )?;
                Ok(())
            })
            .ok();
    }

    fn flush(&self) {}
}
