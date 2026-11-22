//! Klawiatura szesnastkowa niezależna od układu klawiatury hosta.

use std::collections::VecDeque;

pub(super) enum KeyEvent {
    Pressed(u8),
    Released(u8),
}

/// Bieżący stan klawiszy i kolejka zmian dla instrukcji Fx0A.
#[derive(Default)]
pub struct Keypad {
    pressed: [bool; 16],
    events: VecDeque<KeyEvent>,
    capturing: bool,
}

impl Keypad {
    /// Aktualizuje stan klawisza 0–F. Zwraca false dla numeru spoza zakresu.
    /// Powtórzenia tego samego stanu nie tworzą kolejnych zdarzeń.
    pub fn set_key(&mut self, key: u8, pressed: bool) -> bool {
        let Some(state) = self.pressed.get_mut(usize::from(key)) else {
            return false;
        };
        if *state != pressed {
            *state = pressed;
            if self.capturing {
                self.events.push_back(if pressed {
                    KeyEvent::Pressed(key)
                } else {
                    KeyEvent::Released(key)
                });
            }
        }
        true
    }

    /// Informuje, czy wskazany klawisz jest wciśnięty. Numer spoza 0–F daje false.
    pub fn is_pressed(&self, key: u8) -> bool {
        self.pressed.get(usize::from(key)).copied().unwrap_or(false)
    }

    pub(super) fn begin_wait(&mut self) {
        self.events.clear();
        self.capturing = true;
    }

    pub(super) fn end_wait(&mut self) {
        self.events.clear();
        self.capturing = false;
    }

    pub(super) fn next_event(&mut self) -> Option<KeyEvent> {
        self.events.pop_front()
    }
}
