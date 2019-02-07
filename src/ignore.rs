use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::time::SystemTimeError;

#[derive(Debug)]
pub struct Ignore;

impl Display for Ignore {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Ignore")
    }
}

impl Error for Ignore {}

impl From<io::Error> for Ignore {
    fn from(_error: io::Error) -> Self {
        Self
    }
}

impl From<SystemTimeError> for Ignore {
    fn from(_error: SystemTimeError) -> Self {
        Self
    }
}
