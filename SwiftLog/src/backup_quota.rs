// src/backup_quota.rs
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum QuotaError {
    DirTooLarge { current: u64, incoming: u64, max: u64 },
    FsFreeTooSmall { free: u64, incoming: u64, min_free: u64 },
    Io(io::Error),
}
impl From<io::Error> for QuotaError { fn from(e: io::Error) -> Self { QuotaError::Io(e) } }

#[derive(Clone, Copy, Debug)]
pub struct QuotaConfig {
    pub max_dir_bytes: u64,
    pub min_fs_free_bytes: u64,
}

pub fn ensure_backup_quota(dir: &Path, new_file_estimate: u64, cfg: QuotaConfig) -> Result<(), QuotaError> {
    let used = dir_size(dir).unwrap_or(0);

    // 1) 디렉토리 쿼터
    if used.saturating_add(new_file_estimate) > cfg.max_dir_bytes {
        return Err(QuotaError::DirTooLarge {
            current: used,
            incoming: new_file_estimate,
            max: cfg.max_dir_bytes,
        });
    }

    // 2) 파일시스템 여유 공간
    if cfg.min_fs_free_bytes > 0 {
        match fs_free_space_bytes(dir)? {
            Some(free) => {
                if free.saturating_sub(new_file_estimate) < cfg.min_fs_free_bytes {
                    return Err(QuotaError::FsFreeTooSmall {
                        free,
                        incoming: new_file_estimate,
                        min_free: cfg.min_fs_free_bytes,
                    });
                }
            }
            None => {
                // 비윈도우 등: 여유공간 체크 생략(디렉토리 쿼터만 사용)
            }
        }
    }

    Ok(())
}

pub fn dir_size(root: &Path) -> io::Result<u64> {
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut total: u64 = 0;

    while let Some(p) = stack.pop() {
        let rd = match fs::read_dir(&p) {
            Ok(rd) => rd,
            Err(e) => {
                if p == root { return Err(e); } else { continue; }
            }
        };
        for entry in rd {
            let entry = match entry { Ok(e) => e, Err(_) => continue };
            let meta = match entry.metadata() { Ok(m) => m, Err(_) => continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total = total.saturating_add(meta.len());
            } else {
                // symlink 등 무시
            }
        }
    }
    Ok(total)
}

fn fs_free_space_bytes(path: &Path) -> io::Result<Option<u64>> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;

        type BOOL = i32;
        type LPCWSTR = *const u16;

        #[repr(C)]
        struct ULARGE_INTEGER { QuadPart: u64 }

        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: LPCWSTR,
                lpFreeBytesAvailableToCaller: *mut ULARGE_INTEGER,
                lpTotalNumberOfBytes: *mut ULARGE_INTEGER,
                lpTotalNumberOfFreeBytes: *mut ULARGE_INTEGER,
            ) -> BOOL;
        }

        let mut wpath: Vec<u16> = path.as_os_str().encode_wide().collect();
        if !wpath.ends_with(&[0]) { wpath.push(0); }

        let mut free_to_caller = ULARGE_INTEGER { QuadPart: 0 };
        let rv = unsafe {
            GetDiskFreeSpaceExW(
                wpath.as_ptr(),
                &mut free_to_caller as *mut _,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if rv == 0 {
            if let Some(parent) = path.parent() {
                return fs_free_space_bytes(parent);
            }
            return Err(io::Error::last_os_error());
        }
        Ok(Some(free_to_caller.QuadPart))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Ok(None)
    }
}
