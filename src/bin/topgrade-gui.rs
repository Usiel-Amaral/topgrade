#![cfg(unix)]
#![cfg(feature = "gui")]

use eframe::egui;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use regex::Regex;
use rust_i18n::{i18n, t};
use std::env;
use std::io::{Read, Write};
use std::sync::mpsc::{channel, Receiver};
use std::sync::OnceLock;
use std::thread;

i18n!("locales", fallback = "en");

enum AppMsg {
    Output(Vec<u8>),
    Exit,
}

struct ColoredSpan {
    text: String,
    color: Option<egui::Color32>,
    background: Option<egui::Color32>,
    bold: bool,
}

fn parse_ansi(text: &str) -> Vec<ColoredSpan> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\x1b\[([0-9;]*)m").unwrap());

    let mut spans = Vec::new();
    let mut last_idx = 0;
    
    // Default state
    let mut current_color: Option<egui::Color32> = None;
    let mut current_bg: Option<egui::Color32> = None;
    let mut is_bold = false;

    for cap in re.captures_iter(text) {
        let (full_match, codes) = (cap.get(0).unwrap(), cap.get(1).unwrap());
        let start = full_match.start();
        let end = full_match.end();

        if start > last_idx {
            spans.push(ColoredSpan {
                text: text[last_idx..start].to_string(),
                color: current_color,
                background: current_bg,
                bold: is_bold,
            });
        }

        let code_str = codes.as_str();
        if code_str.is_empty() || code_str == "0" {
            current_color = None;
            current_bg = None;
            is_bold = false;
        } else {
            for code in code_str.split(';') {
                match code {
                    "0" => { current_color = None; current_bg = None; is_bold = false; }
                    "1" => is_bold = true,
                    "30" => current_color = Some(egui::Color32::BLACK),
                    "31" => current_color = Some(egui::Color32::RED),
                    "32" => current_color = Some(egui::Color32::GREEN),
                    "33" => current_color = Some(egui::Color32::YELLOW),
                    "34" => current_color = Some(egui::Color32::BLUE),
                    "35" => current_color = Some(egui::Color32::from_rgb(255, 0, 255)), 
                    "36" => current_color = Some(egui::Color32::from_rgb(0, 190, 190)),
                    "37" => current_color = Some(egui::Color32::WHITE),
                    "90" => current_color = Some(egui::Color32::DARK_GRAY),
                    "91" => current_color = Some(egui::Color32::LIGHT_RED),
                    "92" => current_color = Some(egui::Color32::LIGHT_GREEN),
                    "93" => current_color = Some(egui::Color32::LIGHT_YELLOW),
                    "94" => current_color = Some(egui::Color32::LIGHT_BLUE),
                    "95" => current_color = Some(egui::Color32::LIGHT_GRAY), 
                    "96" => current_color = Some(egui::Color32::from_rgb(0, 255, 255)),
                    "97" => current_color = Some(egui::Color32::WHITE),
                    "40" | "41" | "42" | "43" | "44" | "45" | "46" | "47" => { }
                    _ => {}
                }
            }
        }
        last_idx = end;
    }

    if last_idx < text.len() {
        spans.push(ColoredSpan {
            text: text[last_idx..].to_string(),
            color: current_color,
            background: current_bg,
            bold: is_bold,
        });
    }

    spans
}

struct TopgradeApp {
    topgrade_path: String,
    locale: String,
    
    // Terminal state
    running: bool,
    tx_input: Option<Box<dyn Write + Send>>,
    rx_output: Option<Receiver<AppMsg>>,
    
    // Display buffer
    console_lines: Vec<String>, 
    current_line: String,
    cursor_col: usize,    
    
    // User Input
    input_text: String,
    password_mode: bool,
    auto_scroll: bool,
    // We keep track if it *was* running to show the final screen state in the same window
    finished: bool,
}

impl Default for TopgradeApp {
    fn default() -> Self {
        Self {
            topgrade_path: find_topgrade_executable(),
            locale: String::new(),
            running: false,
            tx_input: None,
            rx_output: None,
            console_lines: Vec::new(),
            current_line: String::new(),
            cursor_col: 0,
            input_text: String::new(),
            password_mode: false,
            auto_scroll: true,
            finished: false,
        }
    }
}

impl eframe::App for TopgradeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Collect messages
        let mut loop_break = false;
        let mut messages = Vec::new();
        
        if let Some(rx) = &self.rx_output {
            while let Ok(msg) = rx.try_recv() {
                messages.push(msg);
            }
        }
        
        for msg in messages {
             match msg {
                AppMsg::Output(data) => {
                    let text = String::from_utf8_lossy(&data);
                    self.process_output(&text);
                }
                AppMsg::Exit => {
                    self.running = false;
                    self.finished = true;
                    loop_break = true;
                }
            }
        }
        
        if loop_break {
            self.tx_input = None;
            self.rx_output = None;
        }
        
        // Input handling
        if self.running {
            let events = ctx.input(|i| i.events.clone());
            for event in events {
                if let egui::Event::Text(text) = &event {
                    let clean = text.replace(|c: char| c.is_control(), "");
                    if !clean.is_empty() {
                       self.input_text.push_str(&clean);
                    }
                }
                if let egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } = event {
                    self.input_text.pop();
                }
                if let egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } = event {
                     self.send_input();
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.running && !self.finished {
                // WELCOME SCREEN
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    ui.heading(t!("Topgrade GUI - Title"));
                    ui.add_space(20.0);
                    ui.label(t!("Topgrade GUI - Description"));
                    ui.add_space(30.0);

                    if ui.add(egui::Button::new(t!("Topgrade GUI - Start Button")).min_size(egui::vec2(200.0, 50.0))).clicked() {
                        self.start_topgrade_embedded(ctx.clone());
                    }
                });
            } else {
                // RUNNING / FINISHED SCREEN
                ui.vertical(|ui| {
                    // Header
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Topgrade GUI").strong().color(egui::Color32::WHITE).size(16.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                             if self.running {
                                 ui.spinner();
                                 ui.label("Running...");
                             } else {
                                 ui.label("✅ Done"); // Or localized "Concluído"
                             }
                        });
                    });
                    ui.add_space(5.0);
                    
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgb(30, 30, 30))
                        .inner_margin(10.0)
                        .rounding(5.0)
                        .show(ui, |ui| {
                        
                        let available_height = ui.available_height();
                        egui::ScrollArea::vertical()
                            .max_height(available_height)
                            .stick_to_bottom(self.auto_scroll)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width()); 
                                ui.style_mut().spacing.item_spacing.y = 2.0;
                                
                                let font_id = egui::TextStyle::Monospace.resolve(ui.style());
                                
                                // Render history
                                for line in &self.console_lines {
                                    let spans = parse_ansi(line);
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 0.0;
                                        for span in spans {
                                            let mut text = egui::RichText::new(span.text).font(font_id.clone());
                                            let color = span.color.unwrap_or(egui::Color32::LIGHT_GRAY);
                                            text = text.color(color);
                                            if span.bold { text = text.strong(); }
                                            if let Some(bg) = span.background { text = text.background_color(bg); }
                                            ui.label(text);
                                        }
                                    });
                                }
                                
                                // Render current line + Input
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    let spans = parse_ansi(&self.current_line);
                                    for span in spans {
                                        let mut text = egui::RichText::new(span.text).font(font_id.clone());
                                        let color = span.color.unwrap_or(egui::Color32::WHITE); 
                                        text = text.color(color);
                                        if span.bold { text = text.strong(); }
                                        ui.label(text);
                                    }
                                    
                                    // Input Buffer
                                    if !self.input_text.is_empty() {
                                        let display_text = if self.password_mode {
                                            "*".repeat(self.input_text.len())
                                        } else {
                                            self.input_text.clone()
                                        };
                                        
                                        ui.label(egui::RichText::new(display_text)
                                            .font(font_id.clone())
                                            .color(egui::Color32::GREEN) 
                                        );
                                    }
                                    // Cursor
                                    if self.running && ui.input(|i| i.time % 1.0 < 0.5) {
                                        ui.label(egui::RichText::new("█").font(font_id).color(egui::Color32::GRAY));
                                    }
                                });
                            });
                        });
                });
            }
        });
        
        if self.running {
            ctx.request_repaint();
        }
    }
}

impl TopgradeApp {
    fn start_topgrade_embedded(&mut self, ctx: egui::Context) {
        let topgrade_path = self.topgrade_path.clone();
        
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }).expect("Failed to create PTY");

        let mut cmd = CommandBuilder::new(topgrade_path);
        // Force colors
        cmd.env("DEBIAN_FRONTEND", "readline");
        cmd.env("TERM", "xterm-256color");
        cmd.env("CLICOLOR_FORCE", "1");
        
        let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn topgrade");
        
        let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");
        let (tx_out, rx_out) = channel();
        
        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        let _ = tx_out.send(AppMsg::Output(buf[..n].to_vec()));
                        ctx.request_repaint();
                    }
                    Ok(_) => break, // EOF
                    Err(_) => break,
                }
            }
            let _ = tx_out.send(AppMsg::Exit);
            ctx.request_repaint();
        });

        let writer = pair.master.take_writer().expect("Failed to take writer");

        self.tx_input = Some(writer);
        self.rx_output = Some(rx_out);
        self.running = true;
        self.finished = false;
        self.console_lines.clear();
        self.current_line.clear();
        self.cursor_col = 0;
        self.input_text.clear();
        self.password_mode = false;
    }

    fn process_output(&mut self, text: &str) {
        // Detect password prompt
        let lower = text.to_lowercase();
        // Check for common password prompts
        if (lower.contains("password") || lower.contains("senha") || lower.contains("passphrase")) && text.trim().ends_with(':') {
            self.password_mode = true;
        }

        for c in text.chars() {
            match c {
                '\n' => {
                    self.console_lines.push(std::mem::take(&mut self.current_line));
                    self.cursor_col = 0;
                }
                '\r' => {
                    self.cursor_col = 0;
                }
                c => {
                    if self.cursor_col == 0 && !self.current_line.is_empty() {
                         self.current_line.clear();
                    }
                    self.current_line.push(c);
                    self.cursor_col += 1;
                }
            }
        }
    }

    fn send_input(&mut self) {
        if let Some(tx) = &mut self.tx_input {
            let mut input = std::mem::take(&mut self.input_text);
            input.push('\n'); 
            if let Err(e) = tx.write_all(input.as_bytes()) {
               eprintln!("Failed to write to PTY: {}", e);
            }
            // Always exit password mode after input
            self.password_mode = false;
        }
    }
}

fn find_topgrade_executable() -> String {
    if let Ok(path) = which_crate::which("topgrade") {
        return path.to_string_lossy().to_string();
    }
    if let Ok(exe_path) = env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let p = parent.join("topgrade");
            if p.exists() { return p.to_string_lossy().to_string(); }
        }
         if let Some(workspace_root) = exe_path
            .parent()
            .and_then(|p| p.ancestors().find(|p| p.join("Cargo.toml").exists()))
        {
             let p = workspace_root.join("target/debug/topgrade");
             if p.exists() { return p.to_string_lossy().to_string(); }
             let p = workspace_root.join("target/release/topgrade");
             if p.exists() { return p.to_string_lossy().to_string(); }
        }
    }
    "topgrade".to_string()
}

fn main() -> Result<(), eframe::Error> {
    let system_locale = sys_locale::get_locale().unwrap_or_else(|| "en".to_string());
    let normalized_locale = system_locale.split('.').next().unwrap_or(&system_locale).replace('-', "_");
    rust_i18n::set_locale(&normalized_locale);

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(900.0, 700.0)),
        ..Default::default()
    };

    let mut app = TopgradeApp::default();
    app.locale = normalized_locale;

    eframe::run_native(
        &t!("Topgrade GUI - Title"), 
        options, 
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Box::new(app)
        })
    )
}
