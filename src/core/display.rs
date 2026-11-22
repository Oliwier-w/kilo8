//! Bufor ekranu monochromatycznego CHIP-8.

/// Szerokość ekranu w pikselach.
pub const WIDTH: usize = 64;
/// Wysokość ekranu w pikselach.
pub const HEIGHT: usize = 32;

/// Stan pikseli, zapisany wierszami od lewego górnego rogu.
#[derive(Clone)]
pub struct Display {
    pixels: [bool; WIDTH * HEIGHT],
}

impl Display {
    /// Zwraca wszystkie piksele w kolejności wierszowej.
    pub fn pixels(&self) -> &[bool; WIDTH * HEIGHT] {
        &self.pixels
    }

    /// Gasi wszystkie piksele.
    pub fn clear(&mut self) {
        self.pixels.fill(false);
    }

    /// Rysuje ośmiobitowe wiersze operacją XOR i zawija obie współrzędne.
    /// Zwraca true, jeśli co najmniej jeden zapalony piksel został zgaszony.
    pub(super) fn draw(&mut self, x: u8, y: u8, sprite: &[u8]) -> bool {
        let mut collision = false;
        for (row, &bits) in sprite.iter().enumerate() {
            let py = (usize::from(y) + row) % HEIGHT;
            for col in 0..8 {
                if bits & (0x80 >> col) != 0 {
                    let px = (usize::from(x) + col) % WIDTH;
                    let pixel = &mut self.pixels[py * WIDTH + px];
                    collision |= *pixel;
                    *pixel ^= true;
                }
            }
        }
        collision
    }
}

impl Default for Display {
    fn default() -> Self {
        Self {
            pixels: [false; WIDTH * HEIGHT],
        }
    }
}
