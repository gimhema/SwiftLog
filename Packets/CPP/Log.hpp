
// LogClient.hpp (VS2010 호환)
//  - 배치 직렬화 (MAGIC/VER 관리)
//  - WinSock TCP/UDP 송신
//  - epoch millis 도우미

#pragma once
#define _WINSOCK_DEPRECATED_NO_WARNINGS

#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <string>
#include <vector>

// 링킹: ws2_32.lib
#pragma comment(lib, "ws2_32.lib")

// ===== 고정폭 정수 (VS2010 호환) =====
typedef unsigned char      u8;
typedef unsigned short     u16;
typedef unsigned int       u32;
typedef unsigned __int64   u64;

// ===== 프로토콜 상수 (Rust와 동일하게 맞추세요) =====
static const u32 LOG_MAGIC   = 0x4C4F4750u; // 'LOGP' 예시. Rust의 MAGIC으로 교체!
static const u32 LOG_VERSION = 1u;          // Rust의 VERSION으로 교체!

// ===== LogLevel (Rust와 동일 매핑) =====
enum LogLevel : u8 { Trace=0, Debug=1, Info=2, Warn=3, Error=4 };

// ===== 레코드 표현 =====
struct LogRecord {
    u64 id;       // 일반적으로 0으로 전송 (서버가 부여)
    u64 ts_ms;    // epoch millis
    u8  level;    // LogLevel
    u16 code;     // 앱 정의 코드
    std::string msg_utf8; // UTF-8
    LogRecord() : id(0), ts_ms(0), level(0), code(0) {}
};

// ===== 유틸: 현재 epoch millis =====
static inline u64 now_epoch_millis() {
    // FILETIME(100ns, since 1601) -> Unix epoch(1970)
    FILETIME ft;
    GetSystemTimeAsFileTime(&ft);
    ULARGE_INTEGER uli;
    uli.LowPart  = ft.dwLowDateTime;
    uli.HighPart = ft.dwHighDateTime;
    // 100-ns 단위를 ms로 변환
    const u64 EPOCH_DIFF_100NS = 116444736000000000ULL; // 1970-1601
    u64 t_100ns = uli.QuadPart - EPOCH_DIFF_100NS;
    return t_100ns / 10000ULL; // to ms
}

// ===== 직렬화: 배치 -> 바이트 버퍼 =====
static inline bool serialize_log_batch(const std::vector<LogRecord>& logs,
                                       std::vector<u8>& out_bytes,
                                       u32 magic /*=LOG_MAGIC*/,
                                       u32 version /*=LOG_VERSION*/,
                                       std::string* err /*=0*/)
{
    out_bytes.clear();
    out_bytes.reserve(16 + logs.size() * 64); // 대충 넉넉히

    // 헤더: MAGIC, VERSION (LE)
    // (x86/amd64는 little-endian이므로 그대로 push)
    // u32
    out_bytes.push_back((u8)( magic        & 0xFF));
    out_bytes.push_back((u8)((magic >> 8)  & 0xFF));
    out_bytes.push_back((u8)((magic >> 16) & 0xFF));
    out_bytes.push_back((u8)((magic >> 24) & 0xFF));
    out_bytes.push_back((u8)( version        & 0xFF));
    out_bytes.push_back((u8)((version >> 8)  & 0xFF));
    out_bytes.push_back((u8)((version >> 16) & 0xFF));
    out_bytes.push_back((u8)((version >> 24) & 0xFF));

    // 레코드들
    for (size_t i = 0; i < logs.size(); ++i) {
        const LogRecord& r = logs[i];
        if (r.msg_utf8.size() > 0xFFFF) {
            if (err) *err = "message too long (> 65535)";
            return false;
        }
        u16 msg_len = (u16)r.msg_utf8.size();

        // u64 id
        for (int k=0;k<8;++k) out_bytes.push_back((u8)((r.id    >> (8*k)) & 0xFF));
        // u64 ts_ms
        for (int k=0;k<8;++k) out_bytes.push_back((u8)((r.ts_ms >> (8*k)) & 0xFF));
        // u8 level
        out_bytes.push_back((u8)r.level);
        // u16 code (LE)
        out_bytes.push_back((u8)( r.code       & 0xFF));
        out_bytes.push_back((u8)((r.code >> 8) & 0xFF));
        // u16 msg_len (LE)
        out_bytes.push_back((u8)( msg_len        & 0xFF));
        out_bytes.push_back((u8)((msg_len  >> 8) & 0xFF));
        // msg bytes
        if (msg_len > 0) {
            out_bytes.insert(out_bytes.end(),
                             (const u8*)r.msg_utf8.data(),
                             (const u8*)r.msg_utf8.data() + msg_len);
        }
    }
    return true;
}

// ===== WinSock 초기화/정리 =====
static inline bool winsock_startup(std::string* err /*=0*/) {
    WSADATA wsa;
    int rc = WSAStartup(MAKEWORD(2,2), &wsa);
    if (rc != 0) {
        if (err) { char buf[64]; _snprintf_s(buf, 64, _TRUNCATE, "WSAStartup=%d", rc); *err = buf; }
        return false;
    }
    return true;
}
static inline void winsock_cleanup() {
    WSACleanup();
}

// ===== UDP 송신 =====
static inline bool send_udp_bytes(const char* host, unsigned short port,
                                  const std::vector<u8>& bytes,
                                  std::string* err /*=0*/)
{
    SOCKET s = INVALID_SOCKET;
    bool ok = false;
    do {
        s = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
        if (s == INVALID_SOCKET) { if (err) *err = "socket() failed"; break; }

        sockaddr_in addr;
        ZeroMemory(&addr, sizeof(addr));
        addr.sin_family = AF_INET;
        addr.sin_port   = htons(port);
        addr.sin_addr.s_addr = inet_addr(host);
        if (addr.sin_addr.s_addr == INADDR_NONE) {
            // 도메인 이름인 경우
            hostent* he = gethostbyname(host);
            if (!he) { if (err) *err = "gethostbyname() failed"; break; }
            memcpy(&addr.sin_addr, he->h_addr_list[0], he->h_length);
        }

        int sent = sendto(s, (const char*)&bytes[0], (int)bytes.size(), 0,
                          (sockaddr*)&addr, sizeof(addr));
        if (sent != (int)bytes.size()) { if (err) *err = "sendto() partial/failed"; break; }

        ok = true;
    } while (0);

    if (s != INVALID_SOCKET) closesocket(s);
    return ok;
}

// ===== TCP 송신 =====
static inline bool send_tcp_bytes(const char* host, unsigned short port,
                                  const std::vector<u8>& bytes,
                                  std::string* err /*=0*/)
{
    SOCKET s = INVALID_SOCKET;
    bool ok = false;
    do {
        s = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        if (s == INVALID_SOCKET) { if (err) *err = "socket() failed"; break; }

        sockaddr_in addr;
        ZeroMemory(&addr, sizeof(addr));
        addr.sin_family = AF_INET;
        addr.sin_port   = htons(port);
        addr.sin_addr.s_addr = inet_addr(host);
        if (addr.sin_addr.s_addr == INADDR_NONE) {
            hostent* he = gethostbyname(host);
            if (!he) { if (err) *err = "gethostbyname() failed"; break; }
            memcpy(&addr.sin_addr, he->h_addr_list[0], he->h_length);
        }

        if (connect(s, (sockaddr*)&addr, sizeof(addr)) == SOCKET_ERROR) {
            if (err) *err = "connect() failed"; break;
        }

        // 전체 버퍼 전송
        const char* p = (const char*)&bytes[0];
        int remain = (int)bytes.size();
        while (remain > 0) {
            int n = send(s, p, remain, 0);
            if (n <= 0) { if (err) *err = "send() failed"; break; }
            p += n; remain -= n;
        }
        if (remain == 0) ok = true;

    } while (0);

    if (s != INVALID_SOCKET) closesocket(s);
    return ok;
}
