use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DiskSpace {
    pub total: u64,
    pub free: u64,
}

/// Size and free space of the volume that holds `path`.
#[cfg(windows)]
pub fn disk_space(path: &Path) -> Option<DiskSpace> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some(DiskSpace { total, free: avail })
}

#[cfg(not(windows))]
pub fn disk_space(_path: &Path) -> Option<DiskSpace> {
    None
}
