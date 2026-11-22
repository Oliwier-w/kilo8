# Changelog

## v1.0.0 - 2026-11-22

### Added

- Complete CHIP-8 CPU implementation and 35 core instructions
- COSMAC VIP and SUPER-CHIP compatibility presets
- 64x32 monochrome display with three retro palettes
- CRT Phosphor Persistence and CRT Scanlines post-processing
- Configurable 100-2000 Hz CPU scheduler
- Independent 60 Hz delay and sound timers
- 440 Hz square-wave audio with mute and volume controls
- QWERTY keypad mapping and local keyboard input
- `.ch8` file dialog and drag-and-drop loading
- Live egui debugger for registers, stack, memory, opcode, and quirks
- Demonstration ROM that renders at startup
- Unit coverage for CPU, memory, display, and keypad behavior

### Compatibility

- 12-bit masking for `Bnnn` / `Bxnn` jump targets
- COSMAC VIP key-release handling for `Fx0A`
