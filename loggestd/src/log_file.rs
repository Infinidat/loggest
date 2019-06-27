use bytes::Bytes;
use std::fs::File;
use std::io;
use zstd::stream::copy_encode;

const COMPRESSION_LEVEL: i32 = 6;

pub struct LogFile {
    file: File,
}

impl LogFile {
    pub fn open(path: &str) -> Result<Self, io::Error> {
        let file = File::create(path)?;
        Ok(LogFile { file })
    }

    pub fn write(&mut self, data: &Bytes) -> Result<(), io::Error> {
        copy_encode(data as &[u8], &self.file, COMPRESSION_LEVEL)?;;

        Ok(())
    }
}
