use super::args::Opt;
use super::codec::{LoggestdCodec, LoggestdData::*};
use super::log_file::LogFile;
use futures::prelude::*;
use futures::try_ready;
use log::{info, trace};
use std::default::Default;
use std::fmt::Debug;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::codec::FramedRead;
use tokio::{io::ReadHalf, prelude::*};

enum State {
    Initiated,
    FileOpened(LogFile),
}

impl State {
    fn unwrap_file(&mut self) -> &mut LogFile {
        if let State::FileOpened(f) = self {
            f
        } else {
            panic!("Expected file");
        }
    }

    fn open_file(&mut self, filename: PathBuf) -> Result<(), io::Error> {
        if let State::FileOpened(_) = self {
            panic!("File already opened");
        } else {
            *self = State::FileOpened(LogFile::open(filename)?);
        }

        Ok(())
    }
}

pub struct LoggestdSession<C: AsyncRead + AsyncWrite + Debug> {
    state: State,
    opt: Arc<Opt>,
    reader: FramedRead<ReadHalf<C>, LoggestdCodec>,
}

impl<C: AsyncRead + AsyncWrite + Debug> LoggestdSession<C> {
    pub fn new(connection: C, opt: Arc<Opt>) -> Self {
        let (r, _) = connection.split();
        let reader = FramedRead::new(r, LoggestdCodec::default());
        Self {
            reader,
            opt,
            state: State::Initiated,
        }
    }
}

impl<C: AsyncRead + AsyncWrite + Debug> Future for LoggestdSession<C> {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            if let Some(packet) = try_ready!(self.reader.poll()) {
                trace!("frame: {:x?}", packet);

                match packet {
                    FileName(f) => {
                        self.state.open_file(self.opt.directory.join(f))?;
                    }
                    FileData(data) => {
                        let f = self.state.unwrap_file();
                        f.write(&data)?;
                    }
                };
            } else {
                return Ok(Async::Ready(()));
            }
        }
    }
}

impl<C: AsyncRead + AsyncWrite + Debug> Drop for LoggestdSession<C> {
    fn drop(&mut self) {
        match self.state {
            State::FileOpened(ref f) => {
                info!("Disconnected {}", f.base_filename().display());
            }
            _ => {
                info!("Unnamed session disconnected");
            }
        };
    }
}
