use futures::try_ready;
use log::{debug, error, info};
#[cfg(unix)]
use nix::sys::statvfs::{statvfs, Statvfs};
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use tokio::prelude::*;
use tokio::timer::{Error as TimerError, Interval};

const FREE_SPACE_LOWER_THRESHOLD: f32 = 0.1;
const FREE_SPACE_UPPER_THRESHOLD: f32 = 0.15;

#[derive(Debug)]
struct SpaceData {
    available: u64,
    total: u64,
}

impl SpaceData {
    fn bytes_to_gc(&self) -> Option<u64> {
        if self.available as f32 / self.total as f32 > FREE_SPACE_LOWER_THRESHOLD {
            return None;
        }

        let desired = (self.total as f32 * FREE_SPACE_UPPER_THRESHOLD) as u64;
        debug_assert!(desired > self.available);
        Some(desired - self.available)
    }
}

#[cfg(unix)]
impl From<Statvfs> for SpaceData {
    #[allow(clippy::identity_conversion)]
    fn from(source: Statvfs) -> Self {
        Self {
            available: u64::from(source.blocks_available()) * source.fragment_size(),
            total: u64::from(source.blocks()) * source.fragment_size(),
        }
    }
}

#[cfg(unix)]
fn get_fs_data(directory: &Path) -> Result<SpaceData, io::Error> {
    Ok(statvfs(directory)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        .into())
}

#[cfg(windows)]
fn get_fs_data(directory: &Path) -> Result<SpaceData, io::Error> {
    use widestring::U16CString;
    use winapi::shared::minwindef::FALSE;
    use winapi::um::{errhandlingapi::GetLastError, fileapi, winnt::ULARGE_INTEGER};

    let wstr = U16CString::from_os_str(directory.as_os_str()).unwrap();

    let mut avail: ULARGE_INTEGER = Default::default();
    let mut total: ULARGE_INTEGER = Default::default();
    let mut free: ULARGE_INTEGER = Default::default();

    if FALSE
        == unsafe {
            fileapi::GetDiskFreeSpaceExW(
                wstr.as_ptr(),
                &mut avail as *mut _,
                &mut total as *mut _,
                &mut free as *mut _,
            )
        }
    {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("OS Error: {}", unsafe { GetLastError() }),
        ));
    }

    Ok(SpaceData {
        available: *unsafe { avail.QuadPart() },
        total: *unsafe { total.QuadPart() },
    })
}

pub struct UsageMonitor {
    interval: Interval,
    archive_dir: PathBuf,
}

fn get_entries_with_metadata(directory: &Path) -> Result<Vec<(DirEntry, Option<Metadata>)>, io::Error> {
    let readdir = fs::read_dir(directory)?;

    let mut result: Vec<(DirEntry, Option<Metadata>)> = Vec::with_capacity(readdir.size_hint().1.unwrap_or(0));

    for entry_result in readdir {
        match entry_result {
            Ok(entry) => {
                let metadata = entry
                    .metadata()
                    .map_err(|e| error!("Error reading the metadata of {}: {}", entry.path().display(), e))
                    .ok();

                result.push((entry, metadata));
            }
            Err(e) => error!("Error reading a directory entry: {}", e),
        }
    }

    Ok(result)
}

impl UsageMonitor {
    pub fn new(base_dir: &Path) -> Self {
        let interval = Interval::new(Instant::now(), Duration::from_secs(60));
        UsageMonitor {
            interval,
            archive_dir: base_dir.join("archived"),
        }
    }

    fn garbage_collect(&self) -> Result<(), io::Error> {
        if !self.archive_dir.exists() {
            debug!("Archive dir does not exist");
            return Ok(());
        }

        let fs_data = get_fs_data(&self.archive_dir)?;

        debug!("Filesytem information: {:?}", fs_data);

        if let Some(mut bytes_to_gc) = fs_data.bytes_to_gc() {
            debug!("Need to clean {} bytes", bytes_to_gc);

            let archived_files = {
                let mut l = get_entries_with_metadata(&self.archive_dir)?;
                let now = SystemTime::now();
                l.sort_by(|a, b| {
                    let a_mtime = a.1.as_ref().and_then(|m| m.modified().ok()).unwrap_or(now);
                    let b_mtime = b.1.as_ref().and_then(|m| m.modified().ok()).unwrap_or(now);
                    a_mtime.cmp(&b_mtime)
                });
                l
            };

            for (entry, metadata) in archived_files
                .into_iter()
                .filter_map(|(entry, metadata)| metadata.map(|m| (entry, m)))
            {
                info!(
                    "Deleting {} to free up {} bytes",
                    entry.path().display(),
                    metadata.len()
                );

                match fs::remove_file(entry.path()) {
                    Ok(()) => {
                        if let Some(result) = bytes_to_gc.checked_sub(metadata.len()) {
                            bytes_to_gc = result;
                        } else {
                            debug!("Freed enough space");
                        }
                    }
                    Err(e) => {
                        error!("Cannot remove {}: {}", entry.path().display(), e);
                    }
                }
            }
        } else {
            debug!("No need for GC");
        }

        Ok(())
    }
}

impl Future for UsageMonitor {
    type Item = ();
    type Error = TimerError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            try_ready!(self.interval.poll()).unwrap();

            self.garbage_collect()
                .map_err(|e| error!("Disk usage monitor error: {}", e))
                .ok();
        }
    }
}
