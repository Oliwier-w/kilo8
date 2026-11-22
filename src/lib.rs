//! Rdzeń edukacyjnej maszyny wirtualnej CHIP-8.

pub mod core;

#[cfg(feature = "desktop")]
pub mod audio;

#[cfg(feature = "desktop")]
pub mod ui;
