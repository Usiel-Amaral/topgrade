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

fn clean_ansi(s: &str) -> String {
    // Improved ANSI escape code stripping to include all CSI sequences (colors, cursor moves, etc)
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap());
    re.replace_all(s, "").to_string()
}

struct TopgradeApp {
    topgrade_path: String,
    locale: String,
    
    // Terminal state
    running: bool,
    tx_input: Option<Box<dyn Write + Send>>,
    rx_output: Option<Receiver<Vec<u8>>>,
    
    // Display buffer
    console_lines: Vec<String>, 
    current_line: String,
    cursor_col: usize,    
    
    // User Input
    input_text: String,
    auto_scroll: bool,
    input_visible: bool,
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
            auto_scroll: true,
            input_visible: false,
        }
    }
}

impl eframe::App for TopgradeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Collect incoming data from PTY
        let mut messages = Vec::new();
        if let Some(rx) = &self.rx_output {
            while let Ok(data) = rx.try_recv() {
                messages.push(data);
            }
        }

        // Process messages
        for data in messages {
            let text = String::from_utf8_lossy(&data);
            self.process_output(&text);
        }

        // Feature: Auto-show input if user starts typing while running
        if self.running && !self.input_visible {
            let events = ctx.input(|i| i.events.clone());
            for event in events {
                if let egui::Event::Text(text) = event {
                    if !text.trim().is_empty() {
                        self.input_visible = true;
                        self.input_text.push_str(&text);
                    }
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.running {
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
                // RUNNING SCREEN
                ui.vertical(|ui| {
                    // Header
                    ui.horizontal(|ui| {
                        ui.heading(t!("Topgrade GUI"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spinner();
                            ui.label("Running...");
                            if ui.button("⌨").on_hover_text("Toggle Input").clicked() {
                                self.input_visible = !self.input_visible;
                            }
                        });
                    });
                    
                    ui.separator();

                    // Terminal Output Area
                    let available_height = ui.available_height() - if self.input_visible { 30.0 } else { 0.0 };
                    
                    // Simple container without custom paint to avoid transparency/contrast issues
                    egui::ScrollArea::vertical()
                        .max_height(available_height)
                        .stick_to_bottom(self.auto_scroll)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width()); 
                            ui.style_mut().spacing.item_spacing.y = 0.0;
                            
                            let font_id = egui::TextStyle::Monospace.resolve(ui.style());
                            
                            for line in &self.console_lines {
                                let clean_line = clean_ansi(line);
                                if !clean_line.is_empty() {
                                    ui.label(egui::RichText::new(clean_line).font(font_id.clone()).color(egui::Color32::LIGHT_GRAY));
                                }
                            }
                            
                            let clean_current = clean_ansi(&self.current_line);
                            if !clean_current.is_empty() {
                                    ui.label(egui::RichText::new(clean_current).font(font_id).color(egui::Color32::WHITE));
                            }
                        });
                    
                    ui.separator();

                    // Input Area (Conditional)
                    if self.input_visible {
                        ui.horizontal(|ui| {
                            ui.label("Input:");
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.input_text)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("Type password or command here...")
                                    .password(false) 
                            );

                            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                self.send_input();
                                response.request_focus();
                            }
                            
                            if ui.button("Send").clicked() {
                                self.send_input();
                                response.request_focus();
                            }
                            // Allow closing input manually
                            if ui.button("❌").clicked() {
                                self.input_visible = false;
                            }
                        });
                    }
                });
            }
        });
        
        // Repaint if running
        if self.running {
            ctx.request_repaint();
        }
    }
}

impl TopgradeApp {
    fn start_topgrade_embedded(&mut self, ctx: egui::Context) {
        let topgrade_path = self.topgrade_path.clone();
        
        // Create PTY system
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }).expect("Failed to create PTY");

        let mut cmd = CommandBuilder::new(topgrade_path);
        // Disable colors in child process to reduce garbage and ensure consistent text
        cmd.env("NO_COLOR", "1");
        
        // Spawn the process
        let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn topgrade");
        
        // Setup Reader
        let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");
        let (tx_out, rx_out) = channel();
        
        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        let _ = tx_out.send(buf[..n].to_vec());
                        ctx.request_repaint();
                    }
                    Ok(_) => break, // EOF
                    Err(_) => break,
                }
            }
        });

        // Setup Writer
        let writer = pair.master.take_writer().expect("Failed to take writer");

        self.tx_input = Some(writer);
        self.rx_output = Some(rx_out);
        self.running = true;
        self.console_lines.clear();
        self.current_line.clear();
        self.cursor_col = 0;
    }

    fn process_output(&mut self, text: &str) {
        // Improved heuristic to detect prompts (including localized ones like [S/n])
        let lower = text.to_lowercase();
        if lower.contains("password") || 
           lower.contains("[sudo]") || 
           lower.contains("[y/n]") || 
           lower.contains("[s/n]") || 
           text.trim().ends_with(": ") || 
           text.trim().ends_with("?") {
            self.input_visible = true;
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
            input.push('\n'); // Add Enter
            if let Err(e) = tx.write_all(input.as_bytes()) {
               eprintln!("Failed to write to PTY: {}", e);
            }
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
            // FORCE DARK MODE to ensure high contrast text (White on Dark Gray)
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Box::new(app)
        })
    )
}
