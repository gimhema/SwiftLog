// 공통 상수/유틸
pub const MAGIC: u32 = 0x3147_4C53; // 'SLG1' (LE)
pub const VERSION: u16 = 1;

// 파일 롤링 사이즈(필요시 환경변수/CLI로 확장 가능)
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

// UDP 패킷 최대 수신 크기
pub const UDP_BUF_SIZE: usize = 2048;
