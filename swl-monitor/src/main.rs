mod protocol;
mod sender;
mod tail;

use eframe::{egui, NativeOptions};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::collections::VecDeque;

use protocol::{LogLevel};
use sender::{Transport, start_sending, SendCtrl, SendStats};
use tail::{start_tail, TailCtrl, TailEvent};

fn main() -> eframe::Result<()> {
    let opts = NativeOptions::default();
    eframe::run_native(
        "SwiftLog GUI",
        opts,
        Box::new(|_cc| Box::new(GuiApp::default())),
    )
}

struct GuiApp {
    // 전송 설정
    addr_str: String,
    transport: Transport,
    level_idx: usize,
    code: u16,
    rate: f64,
    msg: String,

    // 런타임
    sender_ctrl: Option<SendCtrl>,
    stats_rx: Option<Receiver<SendStats>>,
    last_stats: SendStats,

    // 모니터링(파일 tail)
    tail_path_str: String,
    tail_ctrl: Option<TailCtrl>,
    lines: VecDeque<String>,
    max_lines: usize,
    errors: Vec<String>,
}

impl Default for GuiApp {
    fn default() -> Self {
        Self {
            addr_str: "127.0.0.1:9501".to_string(), // 기본 UDP 포트
            transport: Transport::Udp,
            level_idx: 2, // Info
            code: 1001,
            rate: 200.0,
            msg: "hello swiftlog".to_string(),

            sender_ctrl: None,
            stats_rx: None,
            last_stats: SendStats { sent: 0, errors: 0 },

            tail_path_str: "logs/app.log".to_string(),
            tail_ctrl: None,
            lines: VecDeque::with_capacity(2000),
            max_lines: 1000,
            errors: Vec::new(),
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.heading("SwiftLog – Test Sender & Live Monitor");
        });

        egui::SidePanel::left("left").resizable(true).show(ctx, |ui| {
            ui.label("Transport");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.transport, Transport::Udp, "UDP");
                ui.selectable_value(&mut self.transport, Transport::Tcp, "TCP");
            });

            ui.separator();
            ui.label("Target Address (ip:port)");
            ui.text_edit_singleline(&mut self.addr_str);

            ui.separator();
            ui.label("Log Level / Code");
            let levels = ["Trace","Debug","Info","Warn","Error"];
            egui::ComboBox::from_label("")
                .selected_text(levels[self.level_idx])
                .show_ui(ui, |ui| {
                    for (i, l) in levels.iter().enumerate() {
                        ui.selectable_value(&mut self.level_idx, i, *l);
                    }
                });
            ui.add(egui::DragValue::new(&mut self.code).clamp_range(0..=u16::MAX));

            ui.separator();
            ui.label("Message Template");
            ui.text_edit_singleline(&mut self.msg);

            ui.separator();
            ui.label("Send Rate (msgs/sec)");
            ui.add(egui::DragValue::new(&mut self.rate).speed(10.0).clamp_range(0.0..=100_000.0));

            ui.horizontal(|ui| {
                if self.sender_ctrl.is_none() {
                    if ui.button("Start Sending").clicked() {
                        self.start_sender();
                    }
                } else {
                    if ui.button("Stop Sending").clicked() {
                        self.stop_sender();
                    }
                }
                ui.label(format!("Sent: {}  Errors: {}", self.last_stats.sent, self.last_stats.errors));
            });

            ui.separator();
            ui.label("Tail Path");
            ui.text_edit_singleline(&mut self.tail_path_str);
            ui.horizontal(|ui| {
                if self.tail_ctrl.is_none() {
                    if ui.button("Start Tail").clicked() {
                        self.start_tail();
                    }
                } else {
                    if ui.button("Stop Tail").clicked() {
                        self.stop_tail();
                    }
                }
                ui.add(egui::DragValue::new(&mut self.max_lines).clamp_range(100..=50_000));
                ui.label("max lines");
            });

            if !self.errors.is_empty() {
                ui.separator();
                ui.colored_label(egui::Color32::RED, "Errors:");
                for e in self.errors.iter().rev().take(6) {
                    ui.label(e);
                }
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Live Log (tail)");
            ui.separator();

            // 수신 라인 갱신
            self.pump_tail_events();

            egui::ScrollArea::vertical().auto_shrink([false,false]).stick_to_bottom(true).show(ui, |ui| {
                for line in &self.lines {
                    ui.label(line);
                }
            });
        });

        // 통계 폴링
        if let Some(rx) = &self.stats_rx {
            for _ in 0..32 {
                if let Ok(s) = rx.try_recv() {
                    self.last_stats = s;
                } else { break; }
            }
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

impl GuiApp {
    fn start_sender(&mut self) {
        let addr: SocketAddr = match self.addr_str.parse() {
            Ok(a) => a,
            Err(e) => { self.errors.push(format!("addr parse error: {}", e)); return; }
        };
        let level = match self.level_idx {
            0 => LogLevel::Trace,
            1 => LogLevel::Debug,
            2 => LogLevel::Info,
            3 => LogLevel::Warn,
            _ => LogLevel::Error,
        };

        let (tx, rx) = mpsc::channel::<SendStats>();
        match start_sending(
            self.transport,
            addr,
            self.msg.clone(),
            level,
            self.code,
            self.rate,
            tx,
        ) {
            Ok(ctrl) => {
                self.sender_ctrl = Some(ctrl);
                self.stats_rx = Some(rx);
            }
            Err(e) => self.errors.push(format!("start_sending error: {}", e)),
        }
    }

    fn stop_sender(&mut self) {
        if let Some(ctrl) = self.sender_ctrl.take() {
            ctrl.stop();
        }
        self.stats_rx = None;
    }

    fn start_tail(&mut self) {
        let path = PathBuf::from(self.tail_path_str.clone());
        match start_tail(path) {
            Ok(ctrl) => { self.tail_ctrl = Some(ctrl); }
            Err(e) => { self.errors.push(format!("tail start error: {}", e)); }
        }
    }

    fn stop_tail(&mut self) {
        if let Some(ctrl) = self.tail_ctrl.take() {
            ctrl.stop();
        }
    }

    fn pump_tail_events(&mut self) {
        if let Some(ctrl) = &self.tail_ctrl {
            for _ in 0..512 {
                match ctrl.rx.try_recv() {
                    Ok(TailEvent::Line(line)) => {
                        self.lines.push_back(line);
                        while self.lines.len() > self.max_lines {
                            self.lines.pop_front();
                        }
                    }
                    Ok(TailEvent::Rotated) => {
                        self.lines.push_back(String::from("--- [rotated] ---"));
                    }
                    Ok(TailEvent::Error(e)) => {
                        self.errors.push(format!("tail error: {}", e));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
        }
    }
}
