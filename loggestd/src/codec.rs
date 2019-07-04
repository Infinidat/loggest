use byteorder::{BigEndian, ByteOrder};
use bytes::{Bytes, BytesMut};
use log::trace;
use std::io;
use std::path::PathBuf;
use std::str::from_utf8;
use tokio::codec::Decoder;

const LENGTH_SIZE: usize = 2;

#[derive(Debug)]
pub enum LoggestdData {
    FileName(PathBuf),
    FileData(Bytes),
}

#[derive(Default, Debug)]
pub struct LoggestdCodec {
    sending_data: bool,
}

impl Decoder for LoggestdCodec {
    type Item = LoggestdData;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        trace!("{:?}", src);
        if !self.sending_data {
            let filename_length = {
                if src.len() < LENGTH_SIZE {
                    return Ok(None);
                }
                BigEndian::read_u16(src.as_ref()) as usize
            };

            if src.len() >= filename_length + LENGTH_SIZE {
                src.split_to(LENGTH_SIZE);
                let buf = src.split_to(filename_length);

                let filename = from_utf8(&buf)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
                    .map(PathBuf::from)?;
                if filename.parent().filter(|s| !s.as_os_str().is_empty()).is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Invalid file name {}:", filename.display()),
                    ));
                }

                self.sending_data = true;
                Ok(Some(LoggestdData::FileName(filename)))
            } else {
                Ok(None)
            }
        } else {
            let buf = src.take();

            Ok(if buf.is_empty() {
                None
            } else {
                Some(LoggestdData::FileData(buf.freeze()))
            })
        }
    }
}
