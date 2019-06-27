use bytes::Bytes;
use log::{debug, info};
use std::fs::{rename, File};
use std::io;
use std::path::{Path, PathBuf};
use zstd::stream::copy_encode;

const COMPRESSION_LEVEL: i32 = 6;
const ARCHIVE_THREASHOLD: usize = 1_073_741_824;

pub struct LogFile {
    file: File,
    filename: PathBuf,
    consumed_data: usize,
    index: usize,
}

impl LogFile {
    pub fn open(mut filename: PathBuf) -> Result<Self, io::Error> {
        let new_extension = filename
            .extension()
            .map(|e| {
                let mut e = e.to_os_string();
                e.push(".1");
                e
            })
            .unwrap_or_else(|| "1".into());
        filename.set_extension(new_extension);

        let file = File::create(&filename)?;
        info!("Opened {}", &filename.display());
        Ok(LogFile {
            file,
            filename,
            consumed_data: 0,
            index: 1,
        })
    }

    fn archive(filename: &Path) -> Result<(), io::Error> {
        let mut archived_path = PathBuf::from("archived");
        archived_path.push(&filename);
        debug!("{} -> {}", filename.display(), archived_path.display());
        rename(&filename, &archived_path)
    }

    fn rotate(&mut self) -> Result<(), io::Error> {
        let old_filename = self.filename.clone();

        self.index += 1;
        self.filename.set_extension(format!("{}", self.index));
        self.file = File::create(&self.filename)?;
        info!("Opened {}", &self.filename.display());
        self.consumed_data = 0;

        LogFile::archive(&old_filename)?;
        Ok(())
    }

    pub fn write(&mut self, data: &Bytes) -> Result<(), io::Error> {
        copy_encode(data as &[u8], &self.file, COMPRESSION_LEVEL)?;;

        self.consumed_data += data.len();
        if self.consumed_data >= ARCHIVE_THREASHOLD {
            self.rotate()?;
        }

        Ok(())
    }

    pub fn filename(&self) -> &Path {
        &self.filename
    }
}

impl Drop for LogFile {
    fn drop(&mut self) {
        LogFile::archive(&self.filename).ok();
    }
}
