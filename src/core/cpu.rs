//! Procesor CHIP-8 i pojedynczy cykl pobrania, dekodowania i wykonania.

use std::fmt;

use super::{
    display::Display,
    keypad::{KeyEvent, Keypad},
    memory::{FONT_START, GLYPH_SIZE, Memory, MemoryError, PROGRAM_START},
    quirks::Quirks,
};

const FLAG: usize = 0xF;

/// Wynik pojedynczego kroku procesora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// Instrukcja zakończyła się.
    Executed,
    /// Fx0A oczekuje na naciśnięcie lub puszczenie wybranego klawisza.
    WaitingForKey,
}

/// Błąd uniemożliwiający wykonanie instrukcji.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuError {
    /// Nie udało się pobrać instrukcji lub uzyskać dostępu do danych.
    Memory(MemoryError),
    /// CALL napotkał pełny stos.
    StackOverflow,
    /// RET napotkał pusty stos.
    StackUnderflow,
    /// Słowo nie ma znaczenia w obsługiwanym zestawie instrukcji.
    UnknownOpcode(u16),
}

impl From<MemoryError> for CpuError {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error)
    }
}

impl fmt::Display for CpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory(error) => error.fmt(f),
            Self::StackOverflow => f.write_str("Przepełnienie stosu CHIP-8"),
            Self::StackUnderflow => f.write_str("Powrót z pustego stosu CHIP-8"),
            Self::UnknownOpcode(opcode) => write!(f, "Nieznany opcode {opcode:#06X}"),
        }
    }
}

impl std::error::Error for CpuError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Memory(error) => Some(error),
            _ => None,
        }
    }
}

struct KeyWait {
    register: usize,
    next_pc: u16,
    key: Option<u8>,
}

/// Rejestry, stos, timery i stan wykonania procesora.
pub struct Cpu {
    /// Rejestry V0–VF. Ostatni rejestr służy także jako flaga.
    pub v: [u8; 16],
    /// Rejestr indeksowy, używany między innymi jako adres danych.
    pub i: u16,
    /// Licznik programu.
    pub pc: u16,
    stack: [u16; 16],
    sp: u8,
    /// Licznik opóźnienia, dekrementowany przez zegar 60 Hz.
    pub delay_timer: u8,
    /// Licznik dźwięku. Wartość większa od zera włącza sygnał dźwiękowy.
    pub sound_timer: u8,
    key_wait: Option<KeyWait>,
}

impl Cpu {
    /// Inicjalizuje procesor z PC ustawionym na 0x200 i wyzerowanym stanem.
    pub fn new() -> Self {
        Self {
            v: [0; 16],
            i: 0,
            pc: PROGRAM_START,
            stack: [0; 16],
            sp: 0,
            delay_timer: 0,
            sound_timer: 0,
            key_wait: None,
        }
    }

    /// Zwraca liczbę aktywnych wpisów stosu.
    pub fn sp(&self) -> u8 {
        self.sp
    }

    /// Zwraca aktywne adresy powrotu od dna do wierzchołka stosu.
    pub fn stack(&self) -> &[u16] {
        &self.stack[..usize::from(self.sp)]
    }

    /// Wykonuje jeden takt obu timerów. Scheduler wywołuje tę metodę z częstotliwością 60 Hz.
    pub fn tick_timers(&mut self) {
        self.delay_timer = self.delay_timer.saturating_sub(1);
        self.sound_timer = self.sound_timer.saturating_sub(1);
    }

    /// Wykonuje instrukcję pod PC. Źródło losowego bajtu jest wywoływane tylko przez Cxnn.
    /// Błąd pozostawia PC przy wadliwej instrukcji. Podczas Fx0A PC pozostaje przy niej
    /// do zwolnienia klawisza naciśniętego po rozpoczęciu oczekiwania.
    /// Przy resecie maszyny zainicjalizuj ponownie CPU i klawiaturę.
    pub fn step(
        &mut self,
        memory: &mut Memory,
        display: &mut Display,
        keypad: &mut Keypad,
        quirks: Quirks,
        random_byte: impl FnOnce() -> u8,
    ) -> Result<StepOutcome, CpuError> {
        if let Some(wait) = &mut self.key_wait {
            while let Some(event) = keypad.next_event() {
                match event {
                    KeyEvent::Pressed(key) if wait.key.is_none() => wait.key = Some(key),
                    KeyEvent::Released(key) if wait.key.is_none() || wait.key == Some(key) => {
                        self.v[wait.register] = key;
                        self.pc = wait.next_pc;
                        self.key_wait = None;
                        keypad.end_wait();
                        return Ok(StepOutcome::Executed);
                    }
                    _ => {}
                }
            }
            return Ok(StepOutcome::WaitingForKey);
        }

        let address = self.pc;
        let opcode = memory.read_opcode(address)?;
        self.pc += 2;
        let outcome = self.execute(opcode, memory, display, keypad, quirks, random_byte);
        if !matches!(outcome, Ok(StepOutcome::Executed)) {
            self.pc = address;
        }
        outcome
    }

    fn execute(
        &mut self,
        opcode: u16,
        memory: &mut Memory,
        display: &mut Display,
        keypad: &mut Keypad,
        quirks: Quirks,
        random_byte: impl FnOnce() -> u8,
    ) -> Result<StepOutcome, CpuError> {
        let n1 = opcode >> 12;
        let n2 = (opcode >> 8) & 0xF;
        let n3 = (opcode >> 4) & 0xF;
        let n4 = opcode & 0xF;
        let nnn = opcode & 0xFFF;
        let nn = opcode as u8;
        let x = usize::from(n2);
        let y = usize::from(n3);
        let n = usize::from(n4);

        // Odczyt przed zapisem flagi ma znaczenie, gdy X lub Y wskazuje VF.
        let vx = self.v[x];
        let vy = self.v[y];
        match (n1, n2, n3, n4) {
            (0, 0, 0xE, 0) => display.clear(),
            (0, 0, 0xE, 0xE) => self.return_from_call()?,
            // SYS to historyczne wywołanie kodu 1802. Kilo8 pomija je w trybie przenośnym.
            (0, ..) => {}
            (1, ..) => self.pc = nnn,
            (2, ..) => self.call(nnn)?,
            (3, ..) => self.skip_if(vx == nn),
            (4, ..) => self.skip_if(vx != nn),
            (5, _, _, 0) => self.skip_if(vx == vy),
            (6, ..) => self.v[x] = nn,
            (7, ..) => self.v[x] = vx.wrapping_add(nn),
            (8, _, _, 0) => self.v[x] = vy,
            (8, _, _, 1) => self.store_logic(x, vx | vy, quirks),
            (8, _, _, 2) => self.store_logic(x, vx & vy, quirks),
            (8, _, _, 3) => self.store_logic(x, vx ^ vy, quirks),
            (8, _, _, 4) => {
                let (result, carry) = vx.overflowing_add(vy);
                self.store_with_flag(x, result, carry);
            }
            (8, _, _, 5) => {
                let (result, borrow) = vx.overflowing_sub(vy);
                self.store_with_flag(x, result, !borrow);
            }
            (8, _, _, 6) => {
                let source = if quirks.shift_uses_vy { vy } else { vx };
                self.store_with_flag(x, source >> 1, source & 1 != 0);
            }
            (8, _, _, 7) => {
                let (result, borrow) = vy.overflowing_sub(vx);
                self.store_with_flag(x, result, !borrow);
            }
            (8, _, _, 0xE) => {
                let source = if quirks.shift_uses_vy { vy } else { vx };
                self.store_with_flag(x, source << 1, source & 0x80 != 0);
            }
            (9, _, _, 0) => self.skip_if(vx != vy),
            (0xA, ..) => self.i = nnn,
            (0xB, ..) => {
                let offset = if quirks.jump_with_vx { vx } else { self.v[0] };
                self.pc = (nnn.wrapping_add(u16::from(offset))) & 0x0FFF;
            }
            (0xC, ..) => self.v[x] = random_byte() & nn,
            (0xD, ..) => {
                let sprite = memory.read_slice(self.i, n)?;
                self.v[FLAG] = u8::from(display.draw(vx, vy, sprite));
            }
            (0xE, _, 9, 0xE) => self.skip_if(keypad.is_pressed(vx)),
            (0xE, _, 0xA, 1) => self.skip_if(!keypad.is_pressed(vx)),
            (0xF, _, 0, 7) => self.v[x] = self.delay_timer,
            (0xF, _, 0, 0xA) => {
                keypad.begin_wait();
                self.key_wait = Some(KeyWait {
                    register: x,
                    next_pc: self.pc,
                    key: None,
                });
                return Ok(StepOutcome::WaitingForKey);
            }
            (0xF, _, 1, 5) => self.delay_timer = vx,
            (0xF, _, 1, 8) => self.sound_timer = vx,
            (0xF, _, 1, 0xE) => self.i = self.i.wrapping_add(u16::from(vx)),
            (0xF, _, 2, 9) => self.i = FONT_START + u16::from(vx & 0xF) * GLYPH_SIZE,
            (0xF, _, 3, 3) => memory.write_slice(self.i, &[vx / 100, vx / 10 % 10, vx % 10])?,
            (0xF, _, 5, 5) => {
                memory.write_slice(self.i, &self.v[..=x])?;
                self.advance_index(n2, quirks);
            }
            (0xF, _, 6, 5) => {
                self.v[..=x].copy_from_slice(memory.read_slice(self.i, x + 1)?);
                self.advance_index(n2, quirks);
            }
            _ => return Err(CpuError::UnknownOpcode(opcode)),
        }
        Ok(StepOutcome::Executed)
    }

    fn skip_if(&mut self, condition: bool) {
        if condition {
            self.pc += 2;
        }
    }

    fn call(&mut self, address: u16) -> Result<(), CpuError> {
        let slot = self
            .stack
            .get_mut(usize::from(self.sp))
            .ok_or(CpuError::StackOverflow)?;
        *slot = self.pc;
        self.sp += 1;
        self.pc = address;
        Ok(())
    }

    fn return_from_call(&mut self) -> Result<(), CpuError> {
        self.sp = self.sp.checked_sub(1).ok_or(CpuError::StackUnderflow)?;
        self.pc = self.stack[usize::from(self.sp)];
        Ok(())
    }

    fn store_logic(&mut self, register: usize, result: u8, quirks: Quirks) {
        self.v[register] = result;
        if quirks.vf_reset_on_logic {
            self.v[FLAG] = 0;
        }
    }

    fn store_with_flag(&mut self, register: usize, result: u8, flag: bool) {
        self.v[register] = result;
        self.v[FLAG] = u8::from(flag);
    }

    fn advance_index(&mut self, x: u16, quirks: Quirks) {
        if quirks.memory_increment_i {
            self.i += x + 1;
        }
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::display::WIDTH;

    #[derive(Default)]
    struct Machine {
        cpu: Cpu,
        ram: Memory,
        screen: Display,
        keys: Keypad,
        quirks: Quirks,
    }

    impl Machine {
        fn step(&mut self) -> Result<StepOutcome, CpuError> {
            self.cpu.step(
                &mut self.ram,
                &mut self.screen,
                &mut self.keys,
                self.quirks,
                || 0xAB,
            )
        }

        fn instruction(&mut self, opcode: u16) -> Result<StepOutcome, CpuError> {
            self.ram
                .write_slice(self.cpu.pc, &opcode.to_be_bytes())
                .unwrap();
            self.step()
        }

        fn run(&mut self, opcode: u16) {
            assert_eq!(self.instruction(opcode), Ok(StepOutcome::Executed));
        }
    }

    #[test]
    fn jump_call_return_and_stack_limits() {
        let mut m = Machine::default();
        m.run(0x1240);
        assert_eq!(m.cpu.pc, 0x240);
        m.run(0x2300);
        assert_eq!(m.cpu.pc, 0x300);
        assert_eq!(m.cpu.stack(), &[0x242]);
        m.run(0x00EE);
        assert_eq!(m.cpu.pc, 0x242);
        assert_eq!(m.instruction(0x00EE), Err(CpuError::StackUnderflow));
        assert_eq!(m.cpu.pc, 0x242);
        for _ in 0..16 {
            m.run(0x2242);
        }
        assert_eq!(m.cpu.sp(), 16);
        assert_eq!(m.step(), Err(CpuError::StackOverflow));
        assert_eq!(m.cpu.pc, 0x242);
        assert_eq!(m.cpu.stack(), &[0x244; 16]);
    }

    #[test]
    fn addition_wraps_and_sets_carry() {
        let mut m = Machine::default();
        m.cpu.v[0] = 255;
        m.cpu.v[1] = 2;
        m.run(0x8014);
        assert_eq!((m.cpu.v[0], m.cpu.v[FLAG]), (1, 1));
        m.run(0x8014);
        assert_eq!((m.cpu.v[0], m.cpu.v[FLAG]), (3, 0));
    }

    #[test]
    fn subtraction_uses_old_vf_and_sets_flag_when_there_is_no_borrow() {
        let mut m = Machine::default();
        for (opcode, vx, vy, result, flag) in [
            (0x8015, 2, 5, 253, 0),
            (0x8015, 5, 5, 0, 1),
            (0x8015, 5, 2, 3, 1),
            (0x8017, 5, 2, 253, 0),
            (0x8017, 5, 5, 0, 1),
            (0x8017, 2, 5, 3, 1),
            (0x80F5, 3, 7, 252, 0),
            (0x80F7, 3, 7, 4, 1),
        ] {
            m.cpu.v[0] = vx;
            m.cpu.v[usize::from((opcode >> 4) & 0xF)] = vy;
            m.run(opcode);
            assert_eq!((m.cpu.v[0], m.cpu.v[FLAG]), (result, flag));
        }
        m.cpu.v[FLAG] = 9;
        m.cpu.v[1] = 2;
        m.run(0x8F15);
        assert_eq!(m.cpu.v[FLAG], 1);
        m.cpu.v[FLAG] = 9;
        m.run(0x8F17);
        assert_eq!(m.cpu.v[FLAG], 0);
    }

    #[test]
    fn shifts_select_operand_by_profile() {
        let mut m = Machine::default();
        for (profile, opcode, result, flag) in [
            (Quirks::COSMAC_VIP, 0x8016, 0x40, 1),
            (Quirks::MODERN, 0x8016, 0x01, 0),
            (Quirks::COSMAC_VIP, 0x801E, 0x02, 1),
            (Quirks::MODERN, 0x801E, 0x04, 0),
        ] {
            m.quirks = profile;
            m.cpu.v[0] = 2;
            m.cpu.v[1] = 0x81;
            m.run(opcode);
            assert_eq!((m.cpu.v[0], m.cpu.v[FLAG]), (result, flag));
            assert_eq!(m.cpu.v[1], 0x81);
        }
    }

    #[test]
    fn logic_with_vf_destination_obeys_quirk() {
        let mut m = Machine::default();
        for (profile, expected) in [(Quirks::COSMAC_VIP, 0), (Quirks::MODERN, 0xA5)] {
            m.quirks = profile;
            m.cpu.v[FLAG] = 0xA0;
            m.cpu.v[1] = 5;
            m.run(0x8F11);
            assert_eq!(m.cpu.v[FLAG], expected);
        }
    }

    #[test]
    fn bcd_preserves_memory_on_out_of_bounds() {
        let mut m = Machine::default();
        m.cpu.i = 0xFFD;
        m.cpu.v[1] = 207;
        m.run(0xF133);
        assert_eq!(m.ram.read_slice(0xFFD, 3).unwrap(), &[2, 0, 7]);
        m.cpu.i = 0xFFE;
        m.cpu.v[1] = 123;
        assert!(matches!(m.instruction(0xF133), Err(CpuError::Memory(_))));
        assert_eq!(m.ram.read_slice(0xFFD, 3).unwrap(), &[2, 0, 7]);
    }

    #[test]
    fn register_transfers_include_vx_and_obey_index_quirk() {
        let mut m = Machine::default();
        for (profile, end) in [(Quirks::COSMAC_VIP, 0x1000), (Quirks::MODERN, 0xFFD)] {
            m.quirks = profile;
            m.cpu.i = 0xFFD;
            m.cpu.v[..4].copy_from_slice(&[1, 2, 3, 4]);
            m.run(0xF255);
            assert_eq!(m.cpu.i, end);
            assert_eq!(m.ram.read_slice(0xFFD, 3).unwrap(), &[1, 2, 3]);
            m.cpu.i = 0xFFD;
            m.cpu.v[..3].fill(0);
            m.run(0xF265);
            assert_eq!(m.cpu.i, end);
            assert_eq!(&m.cpu.v[..4], &[1, 2, 3, 4]);
        }
        m.cpu.i = 0xFFF;
        assert!(matches!(m.instruction(0xF265), Err(CpuError::Memory(_))));
        assert_eq!(&m.cpu.v[..4], &[1, 2, 3, 4]);
    }

    #[test]
    fn draw_wraps_edges_and_reports_xor_collision() {
        let mut m = Machine::default();
        m.cpu.i = 0x300;
        m.ram.write_slice(0x300, &[0xC0, 0x80]).unwrap();
        m.cpu.v[0] = 63;
        m.cpu.v[1] = 31;
        m.run(0xD012);
        assert_eq!(m.cpu.v[FLAG], 0);
        assert!(m.screen.pixels()[31 * WIDTH + 63]);
        assert!(m.screen.pixels()[31 * WIDTH]);
        assert!(m.screen.pixels()[63]);
        m.run(0xD012);
        assert_eq!(m.cpu.v[FLAG], 1);
        assert!(m.screen.pixels().iter().all(|&on| !on));
    }

    #[test]
    fn key_wait_tracks_matching_release_and_keeps_timers_independent() {
        let mut m = Machine::default();
        m.cpu.delay_timer = 2;
        m.cpu.sound_timer = 1;
        assert_eq!(m.instruction(0xF20A), Ok(StepOutcome::WaitingForKey));
        m.keys.set_key(5, true);
        m.keys.set_key(9, true);
        m.keys.set_key(9, false);
        assert_eq!(m.step(), Ok(StepOutcome::WaitingForKey));
        assert_eq!(m.cpu.pc, PROGRAM_START);
        m.cpu.tick_timers();
        m.keys.set_key(5, false);
        assert_eq!(m.step(), Ok(StepOutcome::Executed));
        assert_eq!(m.cpu.pc, PROGRAM_START + 2);
        assert_eq!(m.cpu.v[2], 5);
        assert_eq!((m.cpu.delay_timer, m.cpu.sound_timer), (1, 0));
        m.cpu.tick_timers();
        assert_eq!((m.cpu.delay_timer, m.cpu.sound_timer), (0, 0));
    }

    #[test]
    fn short_key_tap_between_steps_is_recorded() {
        let mut m = Machine::default();
        assert_eq!(m.instruction(0xF20A), Ok(StepOutcome::WaitingForKey));
        m.keys.set_key(0xF, true);
        m.keys.set_key(0xF, false);
        assert_eq!(m.step(), Ok(StepOutcome::Executed));
        assert_eq!(m.cpu.v[2], 0xF);
    }

    #[test]
    fn key_pressed_before_wait_finishes_on_release() {
        let mut m = Machine::default();
        m.keys.set_key(5, true);
        assert_eq!(m.instruction(0xF20A), Ok(StepOutcome::WaitingForKey));
        m.keys.set_key(5, false);
        assert_eq!(m.step(), Ok(StepOutcome::Executed));
        assert_eq!(m.cpu.pc, PROGRAM_START + 2);
        assert_eq!(m.cpu.v[2], 5);
    }

    #[test]
    fn indexed_jump_and_random_mask() {
        let mut m = Machine::default();
        m.cpu.v[0] = 2;
        m.cpu.v[3] = 5;
        m.run(0xB300);
        assert_eq!(m.cpu.pc, 0x302);
        m.quirks = Quirks::MODERN;
        m.run(0xB300);
        assert_eq!(m.cpu.pc, 0x305);
        m.run(0xC30F);
        assert_eq!(m.cpu.v[3], 0xB);
    }

    #[test]
    fn jump_masks_address_to_four_kib() {
        let mut m = Machine::default();
        m.cpu.v[0] = 0x20;
        m.run(0xBFFF);
        assert_eq!(m.cpu.pc, 0x01F);

        m.quirks = Quirks::MODERN;
        m.cpu.v[2] = 0x20;
        m.run(0xB2F0);
        assert_eq!(m.cpu.pc, 0x310);
    }

    #[test]
    fn invalid_opcode_and_failed_fetch_preserve_pc() {
        let mut m = Machine::default();
        assert_eq!(m.instruction(0x5011), Err(CpuError::UnknownOpcode(0x5011)));
        assert_eq!(m.cpu.pc, PROGRAM_START);
        m.cpu.pc = 0xFFF;
        assert!(matches!(m.step(), Err(CpuError::Memory(_))));
        assert_eq!(m.cpu.pc, 0xFFF);
    }
}
