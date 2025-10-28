// main.cpp
#include "LogClient.hpp"
#include <iostream>

int main() {
    std::string err;
    if (!winsock_startup(&err)) {
        std::cout << "WSA init error: " << err << "\n";
        return 1;
    }

    // 로그 2개 배치 만들기
    std::vector<LogRecord> batch;
    LogRecord a;
    a.id = 0; // 서버에서 부여하므로 0 권장
    a.ts_ms = now_epoch_millis();
    a.level = (u8)Info; // Rust LogLevel::Info = 2
    a.code  = 1001;
    a.msg_utf8 = "Service started";
    batch.push_back(a);

    LogRecord b;
    b.id = 0;
    b.ts_ms = now_epoch_millis();
    b.level = (u8)Error; // 4
    b.code  = 5001;
    b.msg_utf8 = "Database connection failed";
    batch.push_back(b);

    // 직렬화
    std::vector<u8> bytes;
    if (!serialize_log_batch(batch, bytes, LOG_MAGIC, LOG_VERSION, &err)) {
        std::cout << "Serialize error: " << err << "\n";
        winsock_cleanup();
        return 1;
    }

    // 전송 (택1) — Rust 에이전트의 수신 포트/프로토콜에 맞추세요
    // UDP:
    // bool ok = send_udp_bytes("127.0.0.1", 9100, bytes, &err);

    // TCP:
    bool ok = send_tcp_bytes("127.0.0.1", 9101, bytes, &err);

    if (!ok) {
        std::cout << "Send error: " << err << "\n";
    } else {
        std::cout << "Sent " << (int)bytes.size() << " bytes\n";
    }

    winsock_cleanup();
    return ok ? 0 : 2;
}
