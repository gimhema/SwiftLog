// src/log_domain.rs
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel { Trace=0, Debug=1, Info=2, Warn=3, Error=4 }

#[derive(Debug, Clone)]
pub struct Log {
    /// 단조 증가하는 내부 ID (삽입 순서, SELECT 정렬 기본 키)
    pub id: u64,
    /// epoch millis
    pub ts_ms: u64,
    pub level: LogLevel,
    pub code: u16,
    pub msg: String,
}

impl Log {
    pub fn new_unassigned(ts_ms: u64, level: LogLevel, code: u16, msg: impl Into<String>) -> Self {
        Self { id: 0, ts_ms, level, code, msg: msg.into() }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.msg.len() > u16::MAX as usize { return Err("message too long"); }
        Ok(())
    }
}
