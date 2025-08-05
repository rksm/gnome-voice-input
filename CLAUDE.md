# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# gnome-voice-input

Background utility for the Linux desktop that transcribes speech using Deepgram and inserts text into currently selected text input field.

## Development Commands

The project uses a `justfile` for task management:

- `just build` - Build the project
- `just run` - Run with debug logging (RUST_LOG=debug)
- `just test` - Run tests with single thread
- `just fmt` - Format code with cargo fmt
- `just lint` - Run clippy with strict warnings
- `just check` - Run format, lint, and test in sequence
- `just release` - Build optimized release version
- `just install` - Install the application system-wide
- `just init-config` - Create default config file
- `just clean` - Clean build artifacts
- `just watch` - Watch for changes and rebuild
- `just deps` - Show system dependencies

### Development Environment

Use `nix develop` to enter the development shell with all dependencies. Or run with `nix develop -c <command>`. E.g. `nix develop -c just check`

## Architecture

### Core Components

- **main.rs**: Application entry point, orchestrates components and handles global hotkey events
- **audio.rs**: Audio capture using cpal, handles microphone input and ring buffer streaming
- **transcription.rs**: Deepgram API integration for speech-to-text, processes audio chunks
- **keyboard.rs**: Text insertion using enigo for cross-platform keyboard simulation
- **hotkey.rs**: Global hotkey registration and management
- **tray.rs**: System tray integration using ksni
- **config.rs**: TOML configuration management with automatic creation

### Key Dependencies

- **cpal**: Cross-platform audio capture
- **deepgram**: Speech-to-text API client
- **enigo**: Cross-platform keyboard/mouse simulation
- **global-hotkey**: System-wide hotkey registration
- **ksni**: KDE StatusNotifierItem (system tray) implementation
- **ringbuf**: Lock-free ring buffer for audio streaming
- **tokio**: Async runtime

### Data Flow

1. Global hotkey triggers recording toggle (src/main.rs:58-65)
2. Audio capture starts in separate thread (src/audio.rs:12)
3. Audio data flows through ring buffer to transcription service
4. Transcriber processes 500ms chunks via Deepgram API (src/transcription.rs:28-31)
5. Transcribed text is automatically typed via enigo (src/keyboard.rs:5)

### Configuration

- Config file: `~/.config/gnome-voice-input/config.toml`
- Default template: `config/default.toml`
- Requires Deepgram API key
- Configurable hotkey (default: Super+V)
- Audio settings (sample rate, channels, buffer size)

### System Dependencies

- `libasound2-dev` / `alsa-lib-devel` for audio
- `libxdo-dev` for X11 keyboard simulation
- GNOME desktop environment recommended

## Important Notes

- Tests run with `--test-threads=1` due to audio device conflicts
- Audio processing happens in dedicated thread to avoid blocking async runtime
- Transcription uses Nova3 model with punctuation enabled
- System tray requires KDE StatusNotifierItem support
