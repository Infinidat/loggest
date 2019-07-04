use crate::ignore::Ignore;
use crate::session;
use crate::{LoggestError, CONFIG};
use log::Record;
use std::cell::RefCell;
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    #[cfg(unix)]
    static OUTPUT: RefCell<Option<session::EstablishedSession<UnixStream>>> = RefCell::new(None);
}

fn get_thread_file(filename: &Path) -> PathBuf {
    let mut os_string = OsString::from(filename.as_os_str());
    os_string.push(format!(".{}", nix::unistd::gettid()));
    os_string.into()
}

pub fn log(record: &Record) {
    OUTPUT
        .with(|output| -> Result<(), Ignore> {
            if output.borrow().is_none() {
                let filename = get_thread_file(unsafe { &CONFIG.as_ref().unwrap().base_filename });
                let session = session::Session::connect_unix()?.establish(filename.to_str().unwrap())?;

                output.replace(Some(session));
            }

            let mut borrow = output.borrow_mut();
            let session = borrow.as_mut().unwrap();

            let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
            let now = now.as_millis() as u64;
            session.write_all(&now.to_le_bytes())?;
            write!(
                session,
                "[{}] {} -- {}\n",
                record.level(),
                record.target(),
                record.args()
            )?;
            Ok(())
        })
        .ok();
}

pub fn initialize_main_thread() -> Result<(), LoggestError> {
    OUTPUT.with(|output| -> Result<(), LoggestError> {
        assert!(output.borrow().is_none());
        let filename = unsafe { &CONFIG.as_ref().unwrap().base_filename };
        let session =
            session::Session::connect_unix()?.establish(filename.to_str().ok_or(LoggestError::BadFileName)?)?;
        output.replace(Some(session));
        Ok(())
    })
}
