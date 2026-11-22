//! Pamięć RAM, font szesnastkowy i bezpieczny dostęp do bajtów CHIP-8.

use std::{fmt, ops::Range};

/// Liczba adresowalnych bajtów RAM.
pub const RAM_SIZE: usize = 4096;
/// Adres pierwszego glifu.
pub const FONT_START: u16 = 0x050;
/// Liczba wierszy jednego glifu.
pub const GLYPH_SIZE: u16 = 5;
/// Adres ładowania programu i początkowa wartość PC.
pub const PROGRAM_START: u16 = 0x200;
/// Maksymalna liczba bajtów obrazu programu.
pub const MAX_ROM_SIZE: usize = RAM_SIZE - PROGRAM_START as usize;

#[rustfmt::skip]
const FONT: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

/// Błąd dostępu do pamięci lub ładowania programu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    /// Początek lub koniec zakresu wykracza poza RAM.
    OutOfBounds {
        /// Adres początku zakresu.
        address: u16,
        /// Żądana liczba bajtów.
        len: usize,
    },
    /// Obraz programu przekracza pojemność obszaru od adresu 0x200.
    RomTooLarge {
        /// Rozmiar obrazu programu w bajtach.
        len: usize,
    },
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds { address, len } => write!(
                f,
                "Zakres od {address:#06X} o długości {len} wykracza poza RAM"
            ),
            Self::RomTooLarge { len } => {
                write!(f, "ROM ma {len} bajtów, limit wynosi {MAX_ROM_SIZE}")
            }
        }
    }
}

impl std::error::Error for MemoryError {}

/// 4 KiB zapisywalnej pamięci. Font znajduje się w tym samym RAM co program.
#[derive(Clone)]
pub struct Memory {
    bytes: [u8; RAM_SIZE],
}

impl Memory {
    /// Tworzy wyzerowany RAM i instaluje 16 glifów pod adresami 0x050–0x09F.
    pub fn new() -> Self {
        let mut bytes = [0; RAM_SIZE];
        let start = usize::from(FONT_START);
        bytes[start..start + FONT.len()].copy_from_slice(&FONT);
        Self { bytes }
    }

    /// Odczytuje pojedynczy bajt.
    pub fn read(&self, address: u16) -> Result<u8, MemoryError> {
        Ok(self.read_slice(address, 1)?[0])
    }

    /// Zapisuje pojedynczy bajt.
    pub fn write(&mut self, address: u16, byte: u8) -> Result<(), MemoryError> {
        self.write_slice(address, &[byte])
    }

    /// Odczytuje słowo w kolejności big-endian, także spod adresu nieparzystego.
    pub fn read_opcode(&self, address: u16) -> Result<u16, MemoryError> {
        let bytes = self.read_slice(address, 2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Udostępnia zakres tylko do odczytu, np. dla sprite'a lub debuggera.
    /// Pusty zakres może zaczynać się tuż za ostatnim bajtem RAM.
    pub fn read_slice(&self, address: u16, len: usize) -> Result<&[u8], MemoryError> {
        Ok(&self.bytes[checked_range(address, len)?])
    }

    /// Zapisuje zakres. Przy błędzie żaden bajt nie zostaje zmieniony.
    pub fn write_slice(&mut self, address: u16, bytes: &[u8]) -> Result<(), MemoryError> {
        self.bytes[checked_range(address, bytes.len())?].copy_from_slice(bytes);
        Ok(())
    }

    /// Zastępuje RAM świeżą pamięcią z fontem i kopiuje obraz od 0x200.
    /// Zbyt duży obraz pozostawia poprzedni stan. Pusty obraz czyści RAM programu.
    pub fn load_rom(&mut self, rom: &[u8]) -> Result<(), MemoryError> {
        if rom.len() > MAX_ROM_SIZE {
            return Err(MemoryError::RomTooLarge { len: rom.len() });
        }
        let mut fresh = Self::new();
        fresh.write_slice(PROGRAM_START, rom)?;
        *self = fresh;
        Ok(())
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

fn checked_range(address: u16, len: usize) -> Result<Range<usize>, MemoryError> {
    let start = usize::from(address);
    let end = start
        .checked_add(len)
        .filter(|&end| end <= RAM_SIZE)
        .ok_or(MemoryError::OutOfBounds { address, len })?;
    Ok(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_font_and_reads_big_endian_opcodes() {
        let mut memory = Memory::new();
        assert_eq!(memory.read_slice(FONT_START, 80).unwrap(), FONT);
        memory.load_rom(&[0xD0, 0x14]).unwrap();
        assert_eq!(memory.read_opcode(PROGRAM_START), Ok(0xD014));
    }

    #[test]
    fn rejects_invalid_ranges_before_writing() {
        let mut memory = Memory::new();
        assert!(memory.write_slice(0xFFF, &[1, 2]).is_err());
        assert_eq!(memory.read(0xFFF), Ok(0));
        assert!(memory.read_opcode(0xFFF).is_err());
        assert!(memory.read_slice(1, usize::MAX).is_err());
    }

    #[test]
    fn rejects_oversized_rom_without_replacing_ram() {
        let mut memory = Memory::new();
        memory.load_rom(&[0x12, 0x34]).unwrap();
        assert!(memory.load_rom(&[0; MAX_ROM_SIZE + 1]).is_err());
        assert_eq!(memory.read_opcode(PROGRAM_START), Ok(0x1234));
        memory.load_rom(&[0xFF; MAX_ROM_SIZE]).unwrap();
        assert_eq!(memory.read(0xFFF), Ok(0xFF));
        memory.load_rom(&[0xAB]).unwrap();
        assert_eq!(memory.read_opcode(PROGRAM_START), Ok(0xAB00));
    }
}
