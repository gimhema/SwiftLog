use std::time::{SystemTime, UNIX_EPOCH};

pub const MAGIC: u32 = 0x3147_4C53; // 'SLG1' (LE)
pub const VERSION: u16 = 1;

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info  = 2,
    Warn  = 3,
    Error = 4,
}

#[inline]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 단일 배치(엔트리 N개)를 SLG1 바이트로 인코딩.
/// entries: (level, code, message)
pub fn encode_batch(entries: &[(LogLevel, u16, &str)]) -> Vec<u8> {
    // 대충 1400바이트 정도 넉넉히
    let mut buf = Vec::with_capacity(1400);
    // BatchHeader
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());

    for (lvl, code, msg) in entries.iter().copied() {
        let ts = now_ms();
        let m = msg.as_bytes();
        let len = (m.len().min(u16::MAX as usize)) as u16;

        buf.extend_from_slice(&ts.to_le_bytes());        // u64
        buf.push(lvl as u8);                              // u8
        buf.extend_from_slice(&code.to_le_bytes());       // u16
        buf.extend_from_slice(&len.to_le_bytes());        // u16
        buf.extend_from_slice(&m[..len as usize]);        // payload
    }

    buf
}
