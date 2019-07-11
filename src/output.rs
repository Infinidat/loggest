use crate::ignore::Ignore;
use crate::session;
use crate::CONFIG;
use log::Record;
use std::cell::RefCell;
use std::ffi::OsString;
use std::io::Write;
#[cfg(windows)]
use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(windows)]
use winapi::um::processthreadsapi::GetCurrentThreadId;

#[cfg(windows)]
type SessionTransport = TcpStream;

#[cfg(unix)]
type SessionTransport = UnixStream;

thread_local! {
    static OUTPUT: RefCell<Option<session::EstablishedSession<SessionTransport>>> = RefCell::new(None);
}

/// Get the system thread ID. The function returns None for the main thread.
fn get_thread_id() -> Option<usize> {
    if std::thread::current().name() == Some("main") {
        return None;
    }

    #[cfg(target_os = "linux")]
    return Some(nix::unistd::gettid().as_raw() as usize);

    #[cfg(all(not(target_os = "linux"), unix))]
    return Some(nix::sys::pthread::pthread_self() as usize);

    #[cfg(windows)]
    return Some(unsafe { GetCurrentThreadId() } as usize);
}

fn get_thread_file(filename: &Path) -> PathBuf {
    let mut os_string = OsString::from(filename.as_os_str());
    if let Some(tid) = get_thread_id() {
        os_string.push(format!(".{}", tid));
    }
    os_string.into()
}

pub fn log(record: &Record) {
    OUTPUT
        .with(|output| -> Result<(), Ignore> {
            if output.borrow().is_none() {
                let filename = get_thread_file(unsafe { &CONFIG.as_ref().unwrap().base_filename });

                let session = session::Session::connect()?.establish(filename.to_str().unwrap())?;

                output.replace(Some(session));
            }

            let mut borrow = output.borrow_mut();
            let session = borrow.as_mut().unwrap();

            let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
            let now = now.as_millis() as u64;
            session.write_all(&now.to_le_bytes())?;
            writeln!(session, "[{}] {} -- {}", record.level(), record.target(), record.args())?;
            Ok(())
        })
        .ok();
}

/// Flush the logger of the current thread
pub fn flush() {
    OUTPUT.with(|output| {
        output.replace(None);
    })
}
