use bytes::Bytes;
use log::{debug, info};
use std::fs::{create_dir, rename, File};
use std::io;
use std::path::{Path, PathBuf};
use zstd::stream::copy_encode;

const COMPRESSION_LEVEL: i32 = 1;
const ARCHIVE_THREASHOLD: usize = 1024 * 1024 * 1024;

pub struct LogFile {
    file: File,
    base_filename: PathBuf,
    consumed_data: usize,
    index: usize,
}

fn generate_filename(base_name: &Path, index: usize) -> PathBuf {
    let mut path = PathBuf::from(base_name);

    let new_filename = format!("{}.{:02}.ioym", path.file_name().unwrap().to_str().unwrap(), index);
    path.set_file_name(new_filename);
    path
}

fn ensure_directory(directory: &Path) -> Result<(), io::Error> {
    let result = create_dir(directory);

    match result {
        Ok(()) => {
            debug!("Created {}", directory.display());
        }
        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }

    Ok(())
}

impl LogFile {
    pub fn open(base_filename: PathBuf) -> Result<Self, io::Error> {
        let index = 1;
        let filename = generate_filename(&base_filename, index);
        let file = File::create(&filename)?;
        info!("Opened {}", filename.display());
        Ok(LogFile {
            file,
            base_filename,
            consumed_data: 0,
            index,
        })
    }

    fn archive(filename: &Path) -> Result<(), io::Error> {
        let archive_directory = filename.parent().unwrap().join("archived");
        ensure_directory(&archive_directory)?;

        let archived_path = archive_directory.join(filename.file_name().unwrap());

        debug!("{} -> {}", filename.display(), archived_path.display());
        rename(&filename, &archived_path)
    }

    fn rotate(&mut self) -> Result<(), io::Error> {
        let old_filename = generate_filename(&self.base_filename, self.index);
        self.index += 1;
        let filename = generate_filename(&self.base_filename, self.index);
        self.file = File::create(&filename)?;
        info!("Opened {}", filename.display());
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

    pub fn base_filename(&self) -> &Path {
        &self.base_filename
    }
}

impl Drop for LogFile {
    fn drop(&mut self) {
        LogFile::archive(&generate_filename(&self.base_filename, self.index)).ok();
    }
}
