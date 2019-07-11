//! `loggest` provides a high performance logging facility for Rust's [log](https://docs.rs/log) crate.
//!
//! Instead of writing logs to a file, `loggest` writes them to a pipe. The other end of the pipe is
//! opened by a daemon which is responsible for writing the logs (and possibly compressing them).

//! # Multithreading
//!
//! Each thread maintains its connection to the log daemon to avoid locking for each log line.

mod ignore;
mod output;
mod session;

use derive_more::From;
use failure::Fail;
use log::{set_logger, set_max_level, LevelFilter, Log, Metadata, Record};
use std::ffi::OsString;
use std::io;

pub use output::flush;

static LOGGER: Loggest = Loggest;
static mut CONFIG: Option<Config> = None;

struct Loggest;

struct Config {
    level: LevelFilter,
    base_filename: OsString,
}

/// Error initializing `loggest`
#[derive(Debug, Fail, From)]
pub enum LoggestError {
    #[fail(display = "I/O error: {}", _0)]
    IoError(#[cause] io::Error),

    #[fail(display = "Set logger error: {}", _0)]
    SetLoggerError(#[cause] log::SetLoggerError),

    #[fail(display = "File name must be a valid utf-8")]
    BadFileName,
}

/// Initialize `loggest`. Must only be called once.
///
/// The `base_filename` argument is used as the name for the main thread. Other threads append `.<thread_id>`.
///
/// # Example
/// ```no_run
/// loggest::init(log::LevelFilter::max(), env!("CARGO_PKG_NAME")).unwrap();
/// ```
pub fn init<P>(level: LevelFilter, base_filename: P) -> Result<FlushGuard, LoggestError>
where
    P: Into<OsString>,
{
    let base_filename: OsString = base_filename.into();

    set_logger(&LOGGER)?;
    set_max_level(level);
    unsafe {
        debug_assert!(CONFIG.is_none());
        CONFIG = Some(Config { level, base_filename });
    }

    Ok(FlushGuard)
}

impl Log for Loggest {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= unsafe { CONFIG.as_ref().unwrap().level }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        output::log(record);
    }

    fn flush(&self) {}
}

pub struct FlushGuard;

impl Drop for FlushGuard {
    fn drop(&mut self) {
        flush();
    }
}
