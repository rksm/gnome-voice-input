# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# gnome-voice-input

Background utility for the Linux desktop that transcribes speech using Deepgram and inserts text into currently selected text input field.

## Development Commands

The project uses a `justfile` for task management:

- `just build` - Build the project
- `just run` - Run with release build and debug logging
- `just debug` - Run debug build with debug logging
- `just test` - Run tests with single thread (required for audio device access)
- `just fmt` - Format code with cargo fmt
- `just lint` - Run clippy with strict warnings
- `just check` - Run format, lint, and test in sequence
- `just install` - Install the application system-wide
- `just init-config` - Create default config file
- `just clean` - Clean build artifacts
- `just watch` - Watch for changes and rebuild
- `just deb` - Build Debian package (requires cargo-deb)
- `just deepgram-costs` - Query Deepgram API costs for last 24 hours

### Development Environment

Use nix dev shell with `nix develop`, run shell commands with `nix develop -c <command>`. E.g. `nix develop -c just check`.

### IMPORTANT DEVELOPMENT NOTES

Always run after making code changes:

- `just check` - Runs fmt, lint, and test
- `just fmt` - Format code

## Architecture

### Core Components

- **main.rs**: Application entry point, orchestrates components and handles global hotkey events
- **audio.rs**: Audio capture using cpal, handles microphone input and ring buffer streaming
- **audio_utils.rs**: Shared audio utilities for different capture scenarios (main app vs examples)
- **transcription.rs**: Deepgram API integration for speech-to-text, processes audio chunks
- **transcription_utils.rs**: Shared transcription utilities and result types
- **keyboard.rs**: Text insertion using enigo for cross-platform keyboard simulation
- **hotkey.rs**: Global hotkey registration and management
- **tray.rs**: System tray integration using ksni
- **config.rs**: TOML configuration management with automatic creation
- **config_watcher.rs**: Live configuration reloading via file system monitoring
- **state.rs**: Shared application state management
- **lib.rs**: Public library API for reusable components

### Key Dependencies

- **cpal**: Cross-platform audio capture
- **deepgram**: Speech-to-text API client
- **enigo**: Cross-platform keyboard/mouse simulation
- **global-hotkey**: System-wide hotkey registration
- **ksni**: KDE StatusNotifierItem (system tray) implementation
- **ringbuf**: Lock-free ring buffer for audio streaming
- **tokio**: Async runtime
- **notify**: File system event monitoring for config hot-reload

### Data Flow

1. Global hotkey triggers recording toggle
2. Audio capture starts in separate thread, samples are sent via channels
3. Audio data is streamed to Deepgram WebSocket for real-time transcription
4. Transcriber returns both interim and final transcription results
5. Transcribed text is automatically typed via enigo keyboard simulation

### Configuration

- Config file: `~/.config/gnome-voice-input/config.toml`
- Default template: `config/default.toml`
- Requires Deepgram API key
- Configurable hotkey (default: Super+V)
- Audio settings (sample rate, channels, buffer size)
- **Live reload**: Configuration changes are automatically detected and applied without restart

### System Dependencies

- `libasound2-dev` / `alsa-lib-devel` for audio
- `libxdo-dev` for X11 keyboard simulation
- GNOME desktop environment recommended

## Technical Notes

- Audio processing happens in dedicated thread to avoid blocking async runtime
- Transcription uses Deepgram Nova3 model with WebSocket streaming for real-time results
- System tray requires KDE StatusNotifierItem support (install AppIndicator extension on GNOME)
- Debug mode (`--debug` flag) saves WAV files of audio chunks sent to Deepgram
- Configuration hot-reloading uses notify crate to watch for file changes
- Graceful shutdown with proper thread termination and resource cleanup
- Library architecture allows shared utilities between main app and examples (see `examples/simple-transcriber.rs`)

### Testing

- Tests must run single-threaded (`--test-threads=1`) due to audio device access limitations
- Use `just test` which automatically handles this requirement

## Important style guide

### Macro imports

We use global macro imports for some crates, like `tracing`. Don't import macros for these crates directly.

Example:

```rust
#[macro_use]
extern crate tracing;
```

✅ Ok to directly use macros:

```rust
info!("This is a log message");
```

❌ Not ok to import macros directly:

```rust
use tracing::info;
info!("This is a log message");
```

### No dead code

Unless specified by the user, never leave dead code in the repository. In particular, after making changes, ensure that all unused functions, variables, and imports are removed.

❌ `#[allow(dead_code)]`

### No lazy imports and exports

- Avoid using `use module_name::*`. Instead, explicitly import only the necessary items.
- Avoid using `pub use module_name::*`. Instead, explicitly re-export only the necessary items.

### Clear exports from sub-systems

For Rust modules that clearly represent sub-systems and their own abstraction layer, don't export types inside the module with `pub use`. Instead have a `mod.rs` file that re-exports the types clearly.
