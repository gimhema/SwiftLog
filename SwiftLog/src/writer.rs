use crate::proto::MAX_FILE_BYTES;
use std::fs::{OpenOptions, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct LogWriter {
    dir: PathBuf,
    base: String,
    file: File,
    written: u64,
}

impl LogWriter {
    pub fn open(dir: &str, base: &str) -> io::Result<Self> {
        let d = PathBuf::from(dir);
        std::fs::create_dir_all(&d)?;
        let path = d.join(format!("{}.log", base));
        let f = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { dir: d, base: base.into(), file: f, written: 0 })
    }

    pub fn rotate_if_needed(&mut self) -> io::Result<()> {
        if self.written < MAX_FILE_BYTES { return Ok(()); }
        drop(&self.file);
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)
            .unwrap_or_default().as_secs();
        let src = self.dir.join(format!("{}.log", self.base));
        let dst = self.dir.join(format!("{}.log.{}", self.base, ts));
        let _ = std::fs::rename(&src, &dst);
        self.file = OpenOptions::new().create(true).append(true)
            .open(self.dir.join(format!("{}.log", self.base)))?;
        self.written = 0;
        Ok(())
    }

    pub fn write_line(&mut self, line: &[u8]) -> io::Result<()> {
        self.file.write_all(line)?;
        self.file.write_all(b"\n")?;
        self.written += (line.len() + 1) as u64;
        Ok(())
    }
}
