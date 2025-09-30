use crate::proto::{MAGIC, VERSION};
use std::convert::TryInto;
use std::io;

// 배치 바이트 → 각 로그 라인을 TSV로 만들어 writer에 전달
pub fn parse_and_write(batch: &[u8], mut sink: impl FnMut(&[u8]) -> io::Result<()>) {
    if batch.len() < 8 { return; }
    if &batch[0..4] != &MAGIC.to_le_bytes() { return; }
    if &batch[4..6] != &VERSION.to_le_bytes() { return; }
    let count = u16::from_le_bytes([batch[6], batch[7]]) as usize;

    let mut p = 8usize;
    for _ in 0..count {
        if p + 8 + 1 + 2 + 2 > batch.len() { break; }
        let ts = u64::from_le_bytes(batch[p..p+8].try_into().unwrap()); p += 8;
        let level = batch[p]; p += 1;
        let code = u16::from_le_bytes(batch[p..p+2].try_into().unwrap()); p += 2;
        let len  = u16::from_le_bytes(batch[p..p+2].try_into().unwrap()) as usize; p += 2;
        if p + len > batch.len() { break; }
        let msg = &batch[p..p+len]; p += len;

        // TSV: ts \t level \t code \t message
        let mut line = Vec::with_capacity(64 + len);
        line.extend_from_slice(ts.to_string().as_bytes());
        line.push(b'\t');
        line.extend_from_slice(level.to_string().as_bytes());
        line.push(b'\t');
        line.extend_from_slice(code.to_string().as_bytes());
        line.push(b'\t');
        line.extend_from_slice(msg);

        // sink에 쓰기(파일 등)
        let _ = sink(&line);
    }
}
