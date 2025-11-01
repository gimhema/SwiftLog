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
    /// 백업 디렉토리의 "총 용량" 상한 (바이트)
    pub max_dir_bytes: u64,
    /// 파일시스템 최소 여유 공간(바이트). 0이면 체크 안 함.
    pub min_fs_free_bytes: u64,
}

pub fn ensure_backup_quota(dir: &Path, new_file_estimate: u64, cfg: QuotaConfig) -> Result<(), QuotaError> {
    let used = dir_size(dir).unwrap_or(0);
    // 1) 디렉터리 자체 쿼터
    if used.saturating_add(new_file_estimate) > cfg.max_dir_bytes {
        return Err(QuotaError::DirTooLarge {
            current: used,
            incoming: new_file_estimate,
            max: cfg.max_dir_bytes,
        });
    }
    // 2) 파일시스템 여유 공간(가능한 플랫폼에서만)
    if cfg.min_fs_free_bytes > 0 {
        if let Some(free) = fs_free_space_bytes(dir).transpose()? {
            if free.saturating_sub(new_file_estimate) < cfg.min_fs_free_bytes {
                return Err(QuotaError::FsFreeTooSmall {
                    free,
                    incoming: new_file_estimate,
                    min_free: cfg.min_fs_free_bytes,
                });
            }
        }
    }
    Ok(())
}

/// 디렉토리의 총 파일 크기(재귀). 실패하는 항목은 건너뛰되, 루트 접근 실패는 Err.
pub fn dir_size(root: &Path) -> io::Result<u64> {
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut total: u64 = 0;

    while let Some(p) = stack.pop() {
        let rd = match fs::read_dir(&p) {
            Ok(rd) => rd,
            Err(e) => {
                // 루트면 에러, 하위면 건너뜀
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
                // symlink 등은 무시
            }
        }
    }
    Ok(total)
}

/// 파일시스템 여유공간(바이트).
/// - Windows: GetDiskFreeSpaceExW 사용
/// - 기타 OS: 현재는 None 반환(디렉터리 쿼터만 사용)
fn fs_free_space_bytes(path: &Path) -> io::Result<Option<u64>> {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
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

        // 경로를 wide string으로
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
            // 경로가 파일 등일 경우 상위 디렉터리로 재시도
            if let Some(parent) = path.parent() {
                return fs_free_space_bytes(parent);
            }
            return Err(io::Error::last_os_error());
        }
        Ok(Some(free_to_caller.QuadPart))
    }

    #[cfg(not(target_os = "windows"))]
    {
        // 외부 크레이트 없이 이식성 보장을 위해 기본은 None.
        // (원하면 statvfs FFI를 추가해 리눅스/맥도 지원 가능)
        let _ = path;
        Ok(None)
    }
}
