//! Przełączniki zgodności dla czterech różnic między interpreterami CHIP-8.

/// Reguły przekazywane procesorowi podczas wykonywania instrukcji.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quirks {
    /// 8xy6 i 8xyE pobierają operand z VY, gdy przełącznik jest włączony.
    /// Przy wyłączonym pobierają go z VX. Wynik trafia do VX, a wysunięty bit do VF.
    pub shift_uses_vy: bool,

    /// Fx55 i Fx65 zwiększają I o x + 1 po pomyślnym transferze.
    /// Przy wyłączonym przełączniku pozostawiają I bez zmian.
    pub memory_increment_i: bool,

    /// Bxnn skacze pod nnn + VX, gdzie x jest górnym niblem adresu nnn.
    /// Przy wyłączonym przełączniku Bnnn dodaje V0 do nnn.
    pub jump_with_vx: bool,

    /// 8xy1, 8xy2 i 8xy3 odczytują oba operandy, zapisują wynik do VX, a potem zerują VF.
    /// Gdy x = F, końcowy zapis flagi zastępuje wynik operacji zerem.
    /// Zero jest regułą zgodności Kilo8. Oryginalny interpreter kopiował bit DF procesora 1802.
    /// Przy wyłączonym przełączniku jedyny zapis to wynik w VX, także gdy x = F.
    pub vf_reset_on_logic: bool,
}

impl Quirks {
    /// Zachowania COSMAC VIP w zakresie czterech dostępnych przełączników.
    pub const COSMAC_VIP: Self = Self {
        shift_uses_vy: true,
        memory_increment_i: true,
        jump_with_vx: false,
        vf_reset_on_logic: true,
    };

    /// Profil z przesuwaniem VX, skokiem Bxnn i zachowaniem I przy transferze.
    pub const MODERN: Self = Self {
        shift_uses_vy: false,
        memory_increment_i: false,
        jump_with_vx: true,
        vf_reset_on_logic: false,
    };
}

impl Default for Quirks {
    /// Ustawia cztery przełączniki zgodnie z profilem COSMAC VIP.
    fn default() -> Self {
        Self::COSMAC_VIP
    }
}
