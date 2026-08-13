use std::{fs, io, path::Path, time::SystemTime};

/// Metadata captured immediately before and after one source read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileStamp {
    pub(crate) length: u64,
    pub(crate) modified: SystemTime,
    pub(crate) executable_mask: u8,
}

/// Inspects a path without following a final symlink.
pub(crate) fn regular_file_stamp(path: &Path) -> io::Result<Option<FileStamp>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Ok(None);
    }
    Ok(Some(FileStamp {
        length: metadata.len(),
        modified: metadata.modified()?,
        executable_mask: executable_mask(&metadata),
    }))
}

#[cfg(unix)]
fn executable_mask(metadata: &fs::Metadata) -> u8 {
    use std::os::unix::fs::PermissionsExt;

    let mode = metadata.permissions().mode();
    (u8::from(mode & 0o100 != 0) << 2)
        | (u8::from(mode & 0o010 != 0) << 1)
        | u8::from(mode & 0o001 != 0)
}

#[cfg(not(unix))]
const fn executable_mask(_: &fs::Metadata) -> u8 {
    0
}

/// Replaces only the three Unix execute bits; other permission bits survive the write.
#[cfg(unix)]
pub(crate) fn restore_executable_mask(path: &Path, mask: u8) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    let execute = ((u32::from(mask & 0b100) >> 2) * 0o100)
        | ((u32::from(mask & 0b010) >> 1) * 0o010)
        | u32::from(mask & 0b001);
    permissions.set_mode((permissions.mode() & !0o111) | execute);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
pub(crate) fn restore_executable_mask(_: &Path, mask: u8) -> io::Result<()> {
    if mask == 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "non-Unix snapshots cannot carry executable masks",
        ))
    }
}
