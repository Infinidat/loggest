use super::codec::{LoggestdCodec, LoggestdData::*};
use futures::prelude::*;
use futures::try_ready;
use log::{info, trace};
use std::default::Default;
use std::fmt::Debug;
use std::fs::File;
use std::io;
use tokio::codec::FramedRead;
use tokio::{io::ReadHalf, prelude::*};

enum State {
    Initiated,
    FileOpened(File),
}

impl State {
    fn unwrap_file(&mut self) -> &mut File {
        if let State::FileOpened(f) = self {
            f
        } else {
            panic!("Expected file");
        }
    }

    fn open_file(&mut self, path: &str) -> Result<(), io::Error> {
        if let State::FileOpened(_) = self {
            panic!("File already opened");
        } else {
            *self = State::FileOpened(File::create(&path)?);
            info!("Opened {}", path);
        }

        Ok(())
    }
}

pub struct LoggestdSession<C: AsyncRead + AsyncWrite + Debug> {
    state: State,
    reader: FramedRead<ReadHalf<C>, LoggestdCodec>,
}

impl<C: AsyncRead + AsyncWrite + Debug> LoggestdSession<C> {
    pub fn new(connection: C) -> Self {
        let (r, _) = connection.split();
        let reader = FramedRead::new(r, LoggestdCodec::default());
        Self {
            reader,
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
                        self.state.open_file(&f)?;
                    }
                    FileData(data) => {
                        let f = self.state.unwrap_file();
                        f.write_all(&data)?;
                    }
                };
            } else {
                return Ok(Async::Ready(()));
            }
        }
    }
}
