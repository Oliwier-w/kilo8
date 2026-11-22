use std::{
    fs,
    time::{Duration, Instant},
};

use eframe::egui;

use crate::{
    audio::Audio,
    core::{
        cpu::{Cpu, StepOutcome},
        display::{Display, HEIGHT, WIDTH},
        keypad::Keypad,
        memory::Memory,
        quirks::Quirks,
    },
};

#[derive(Clone, Copy, PartialEq)]
enum Palette {
    Classic,
    Phosphor,
    Amber,
}

fn blend_color(off: egui::Color32, on: egui::Color32, brightness: f32) -> egui::Color32 {
    let brightness = brightness.clamp(0.0, 1.0);
    egui::Color32::from_rgb(
        (f32::from(off.r()) + (f32::from(on.r()) - f32::from(off.r())) * brightness) as u8,
        (f32::from(off.g()) + (f32::from(on.g()) - f32::from(off.g())) * brightness) as u8,
        (f32::from(off.b()) + (f32::from(on.b()) - f32::from(off.b())) * brightness) as u8,
    )
}

fn demo_rom() -> Vec<u8> {
    let mut rom = Vec::new();
    for (digit, x) in [('K', 8), ('I', 16), ('L', 24), ('O', 32), ('8', 40)] {
        rom.extend([0x60, x]);
        rom.extend([0x61, 8]);
        rom.extend([
            0x6A,
            match digit {
                'K' => 0x0A,
                'I' => 0x09,
                'L' => 0x0B,
                'O' => 0x0C,
                '8' => 0x08,
                _ => 0,
            },
        ]);
        rom.extend([0xFA, 0x29, 0xD0, 0x15]);
    }
    rom.extend([0x12, 0x00]);
    rom
}

pub struct Kilo8App {
    cpu: Cpu,
    memory: Memory,
    display: Display,
    keypad: Keypad,
    quirks: Quirks,
    paused: bool,
    cpu_hz: u32,
    palette: Palette,
    muted: bool,
    volume: f32,
    audio: Audio,
    last_frame: Instant,
    cpu_accumulator: Duration,
    timer_accumulator: Duration,
    phosphor_buffer: [f32; WIDTH * HEIGHT],
    phosphor_ghosting: bool,
    crt_scanlines: bool,
}

impl Kilo8App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut memory = Memory::new();
        let _ = memory.load_rom(&demo_rom());
        Self {
            cpu: Cpu::new(),
            memory,
            display: Display::default(),
            keypad: Keypad::default(),
            quirks: Quirks::COSMAC_VIP,
            paused: false,
            cpu_hz: 700,
            palette: Palette::Classic,
            muted: false,
            volume: 0.35,
            audio: Audio::new(),
            last_frame: Instant::now(),
            cpu_accumulator: Duration::ZERO,
            timer_accumulator: Duration::ZERO,
            phosphor_buffer: [0.0; WIDTH * HEIGHT],
            phosphor_ghosting: true,
            crt_scanlines: true,
        }
    }

    fn reset(&mut self) {
        self.cpu = Cpu::new();
        self.display.clear();
        self.keypad = Keypad::default();
        self.phosphor_buffer.fill(0.0);
    }

    fn load_rom(&mut self, rom: &[u8]) {
        if self.memory.load_rom(rom).is_ok() {
            self.reset();
        }
    }

    fn opcode_mnemonic(&self) -> String {
        let Ok(opcode) = self.memory.read_opcode(self.cpu.pc) else {
            return "----".to_owned();
        };
        let n1 = opcode >> 12;
        let n2 = (opcode >> 8) & 0xF;
        let n3 = (opcode >> 4) & 0xF;
        let n4 = opcode & 0xF;
        let nnn = opcode & 0xFFF;
        let nn = opcode as u8;
        match (n1, n2, n3, n4) {
            (0, 0, 0xE, 0) => "CLS".to_owned(),
            (0, 0, 0xE, 0xE) => "RET".to_owned(),
            (1, ..) => format!("JP {nnn:#05X}"),
            (2, ..) => format!("CALL {nnn:#05X}"),
            (3, ..) => format!("SE V{n2:X}, {nn:#04X}"),
            (4, ..) => format!("SNE V{n2:X}, {nn:#04X}"),
            (5, _, _, 0) => format!("SE V{n2:X}, V{n3:X}"),
            (6, ..) => format!("LD V{n2:X}, {nn:#04X}"),
            (7, ..) => format!("ADD V{n2:X}, {nn:#04X}"),
            (8, _, _, 0) => format!("LD V{n2:X}, V{n3:X}"),
            (8, _, _, 1) => format!("OR V{n2:X}, V{n3:X}"),
            (8, _, _, 2) => format!("AND V{n2:X}, V{n3:X}"),
            (8, _, _, 3) => format!("XOR V{n2:X}, V{n3:X}"),
            (8, _, _, 4) => format!("ADD V{n2:X}, V{n3:X}"),
            (8, _, _, 5) => format!("SUB V{n2:X}, V{n3:X}"),
            (8, _, _, 6) => format!("SHR V{n2:X}"),
            (8, _, _, 7) => format!("SUBN V{n2:X}, V{n3:X}"),
            (8, _, _, 0xE) => format!("SHL V{n2:X}"),
            (9, _, _, 0) => format!("SNE V{n2:X}, V{n3:X}"),
            (0xA, ..) => format!("LD I, {nnn:#05X}"),
            (0xB, ..) => format!("JP V0, {nnn:#05X}"),
            (0xC, ..) => format!("RND V{n2:X}, {nn:#04X}"),
            (0xD, ..) => format!("DRW V{n2:X}, V{n3:X}, {n4}"),
            (0xE, _, 9, 0xE) => format!("SKP V{n2:X}"),
            (0xE, _, 0xA, 1) => format!("SKNP V{n2:X}"),
            (0xF, _, 0, 7) => format!("LD V{n2:X}, DT"),
            (0xF, _, 0, 0xA) => format!("LD V{n2:X}, K"),
            (0xF, _, 1, 5) => format!("LD DT, V{n2:X}"),
            (0xF, _, 1, 8) => format!("LD ST, V{n2:X}"),
            (0xF, _, 1, 0xE) => format!("ADD I, V{n2:X}"),
            (0xF, _, 2, 9) => format!("LD F, V{n2:X}"),
            (0xF, _, 3, 3) => format!("LD B, V{n2:X}"),
            (0xF, _, 5, 5) => format!("LD [I], V{n2:X}"),
            (0xF, _, 6, 5) => format!("LD V{n2:X}, [I]"),
            _ => format!("{opcode:#06X}"),
        }
    }

    fn update_keys(&mut self, ctx: &egui::Context) {
        let mapping = [
            (egui::Key::Num1, 1),
            (egui::Key::Num2, 2),
            (egui::Key::Num3, 3),
            (egui::Key::Num4, 0xC),
            (egui::Key::Q, 4),
            (egui::Key::W, 5),
            (egui::Key::E, 6),
            (egui::Key::R, 0xD),
            (egui::Key::A, 7),
            (egui::Key::S, 8),
            (egui::Key::D, 9),
            (egui::Key::F, 0xE),
            (egui::Key::Z, 0xA),
            (egui::Key::X, 0),
            (egui::Key::C, 0xB),
            (egui::Key::V, 0xF),
        ];
        ctx.input(|input| {
            for (key, chip_key) in mapping {
                self.keypad.set_key(chip_key, input.key_down(key));
            }
        });
    }

    fn render_screen(&self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let scale = ((avail.x / WIDTH as f32).min(avail.y / HEIGHT as f32))
            .floor()
            .max(1.0);
        let size = egui::Vec2::splat(scale);
        let (off_x, off_y) = ((avail.x - WIDTH as f32 * scale) * 0.5, 8.0);
        let (on, off) = match self.palette {
            Palette::Classic => (egui::Color32::WHITE, egui::Color32::BLACK),
            Palette::Phosphor => (
                egui::Color32::from_rgb(85, 255, 128),
                egui::Color32::from_rgb(5, 20, 10),
            ),
            Palette::Amber => (
                egui::Color32::from_rgb(255, 176, 64),
                egui::Color32::from_rgb(24, 12, 2),
            ),
        };
        let painter = ui.painter();
        painter.rect_filled(
            egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(WIDTH as f32 * scale, HEIGHT as f32 * scale),
            ),
            0.0,
            off,
        );
        for (index, &brightness) in self.phosphor_buffer.iter().enumerate() {
            let x = index % WIDTH;
            let y = index / WIDTH;
            if brightness <= 0.0 {
                continue;
            }
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    ui.cursor().min.x + off_x + x as f32 * scale,
                    ui.cursor().min.y + off_y + y as f32 * scale,
                ),
                size,
            );
            let color = blend_color(off, on, brightness);
            painter.rect_filled(rect, 0.0, color);
        }
        if self.crt_scanlines {
            for y in (1..HEIGHT).step_by(2) {
                let rect = egui::Rect::from_min_size(
                    egui::pos2(
                        ui.cursor().min.x + off_x,
                        ui.cursor().min.y + off_y + y as f32 * scale,
                    ),
                    egui::vec2(WIDTH as f32 * scale, scale),
                );
                painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(42));
            }
        }
        ui.allocate_space(egui::vec2(
            WIDTH as f32 * scale,
            HEIGHT as f32 * scale + 16.0,
        ));
    }
}

impl eframe::App for Kilo8App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;

        let frame_decay = if self.phosphor_ghosting {
            (-elapsed.as_secs_f32() * 4.8).exp()
        } else {
            0.0
        };
        for (index, pixel) in self.display.pixels().iter().enumerate() {
            if *pixel {
                self.phosphor_buffer[index] = 1.0;
            } else if self.phosphor_ghosting {
                self.phosphor_buffer[index] *= frame_decay;
                if self.phosphor_buffer[index] < 0.01 {
                    self.phosphor_buffer[index] = 0.0;
                }
            } else {
                self.phosphor_buffer[index] = 0.0;
            }
        }

        self.update_keys(&ctx);

        if let Some(path) = ctx
            .input(|i| i.raw.dropped_files.clone())
            .into_iter()
            .map(|file| file.path().to_path_buf())
            .next()
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ch8"))
            })
            && let Ok(rom) = fs::read(path)
        {
            self.load_rom(&rom);
        }

        if !self.paused {
            self.cpu_accumulator = self.cpu_accumulator.saturating_add(elapsed);
            self.timer_accumulator = self.timer_accumulator.saturating_add(elapsed);
            let cpu_step = Duration::from_secs_f64(1.0 / f64::from(self.cpu_hz));
            let mut steps = 0u16;
            while self.cpu_accumulator >= cpu_step && steps < 20 {
                self.cpu_accumulator -= cpu_step;
                let random = rand::random();
                if let Ok(StepOutcome::WaitingForKey) = self.cpu.step(
                    &mut self.memory,
                    &mut self.display,
                    &mut self.keypad,
                    self.quirks,
                    || random,
                ) {
                    self.paused = false;
                }
                steps += 1;
            }
            let timer_step = Duration::from_secs_f64(1.0 / 60.0);
            while self.timer_accumulator >= timer_step {
                self.timer_accumulator -= timer_step;
                self.cpu.tick_timers();
            }
        }

        self.audio.set_muted(self.muted);
        self.audio.set_volume(self.volume);
        self.audio.set_active(self.cpu.sound_timer > 0);

        egui::containers::Panel::right("debugger")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| {
                ui.heading("Debugger");
                ui.horizontal(|ui| {
                    if ui
                        .button(if self.paused { "Resume" } else { "Pause" })
                        .clicked()
                    {
                        self.paused = !self.paused;
                    }
                    if ui.button("Step").clicked() && self.paused {
                        let _ = self.cpu.step(
                            &mut self.memory,
                            &mut self.display,
                            &mut self.keypad,
                            self.quirks,
                            rand::random,
                        );
                    }
                    if ui.button("Reset").clicked() {
                        self.reset();
                    }
                });
                ui.add(egui::Slider::new(&mut self.cpu_hz, 100..=2000).text("CPU Hz"));

                ui.separator();
                ui.heading("Registers");
                for (index, value) in self.cpu.v.iter().enumerate() {
                    ui.label(format!("V{index:X}: {value:#04X}"));
                }
                ui.label(format!("I: {:#05X}", self.cpu.i));
                ui.label(format!("PC: {:#05X}", self.cpu.pc));
                ui.label(format!("SP: {}", self.cpu.sp()));
                ui.label(format!("DT: {}", self.cpu.delay_timer));
                ui.label(format!("ST: {}", self.cpu.sound_timer));
                let opcode = self.memory.read_opcode(self.cpu.pc).unwrap_or(0);
                ui.label(format!("Opcode: {opcode:#06X}"));
                ui.label(format!("Mnemonic: {}", self.opcode_mnemonic()));

                ui.separator();
                ui.heading("Stack");
                ui.label(format!("Depth: {}", self.cpu.sp()));
                for (depth, address) in self.cpu.stack().iter().enumerate() {
                    ui.label(format!("{depth}: {address:#05X}"));
                }

                ui.separator();
                ui.heading("Memory");
                for base in [
                    self.cpu.pc.saturating_sub(8) & 0x0FFF,
                    self.cpu.i.saturating_sub(8) & 0x0FFF,
                ] {
                    let end = (base as usize).saturating_add(16).min(4096);
                    if let Ok(bytes) = self.memory.read_slice(base, end - base as usize) {
                        ui.label(format!(
                            "{base:#05X}: {}",
                            bytes
                                .iter()
                                .map(|b| format!("{b:02X}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ));
                    }
                }

                ui.separator();
                ui.heading("Quirks");
                ui.checkbox(&mut self.quirks.shift_uses_vy, "shift_uses_vy");
                ui.checkbox(&mut self.quirks.memory_increment_i, "memory_increment_i");
                ui.checkbox(&mut self.quirks.jump_with_vx, "jump_with_vx");
                ui.checkbox(&mut self.quirks.vf_reset_on_logic, "vf_reset_on_logic");
                if ui.button("COSMAC VIP").clicked() {
                    self.quirks = Quirks::COSMAC_VIP;
                }
                if ui.button("SUPER-CHIP").clicked() {
                    self.quirks = Quirks::MODERN;
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Kilo8");
            ui.horizontal(|ui| {
                if ui.button("Load .ch8").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("CHIP-8 ROM", &["ch8"])
                        .pick_file()
                    && let Ok(rom) = fs::read(path)
                {
                    self.load_rom(&rom);
                }
                egui::ComboBox::from_label("Palette")
                    .selected_text(match self.palette {
                        Palette::Classic => "Classic Mono",
                        Palette::Phosphor => "Phosphor Green",
                        Palette::Amber => "Amber CRT",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.palette, Palette::Classic, "Classic Mono");
                        ui.selectable_value(&mut self.palette, Palette::Phosphor, "Phosphor Green");
                        ui.selectable_value(&mut self.palette, Palette::Amber, "Amber CRT");
                    });
                ui.checkbox(&mut self.muted, "Mute");
                ui.add(egui::Slider::new(&mut self.volume, 0.0..=1.0).text("Volume"));
                ui.checkbox(&mut self.phosphor_ghosting, "Phosphor Ghosting");
                ui.checkbox(&mut self.crt_scanlines, "CRT Scanlines");
            });
            self.render_screen(ui);
        });

        ctx.request_repaint();
    }
}
