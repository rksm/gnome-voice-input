# GNOME Voice Input

Voice-to-text utility for Linux desktop that transcribes speech using Deepgram and inserts text into any focused field.

## Features

- **Real-time transcription** using Deepgram Nova3 model
- **Global hotkey** to start/stop recording (default: Super+V)
- **System tray icon** with recording status indicator
- **Live config reload** - changes apply without restart
- **Auto text insertion** into any focused text field
- **Debug mode** saves audio chunks as WAV files
- **Graceful shutdown** with proper resource cleanup
- **Multi-format support** - smart formatting for numbers, dates, punctuation

## Quick Start

### Prerequisites

```bash
# Debian/Ubuntu
sudo apt install libasound2-dev libxdo-dev

# Fedora
sudo dnf install alsa-lib-devel libxdo-devel
```

Get a Deepgram API key at [console.deepgram.com](https://console.deepgram.com/)

### Install

```bash
# Clone and build
git clone https://github.com/yourusername/gnome-voice-input.git
cd gnome-voice-input
just install

# Setup config
just init-config
# Add your Deepgram API key to ~/.config/gnome-voice-input/config.toml
```

### Nix Installation

```bash
# Run directly
nix run github:yourusername/gnome-voice-input

# Install to profile
nix profile install github:yourusername/gnome-voice-input

# Development
nix develop
nix develop -c just check
```

## Usage

```bash
# Start normally
gnome-voice-input

# With custom config
gnome-voice-input --config /path/to/config.toml

# Debug mode (saves audio as WAV files)
gnome-voice-input --debug
```

Press **Super+V** to start/stop recording. Transcribed text is automatically typed into the focused field.

## Configuration

Config at `~/.config/gnome-voice-input/config.toml` (live-reloads on change):

```toml
deepgram_api_key = "your-api-key-here"

[hotkey]
modifiers = ["super"]  # super, ctrl, alt, shift
key = "v"

[audio]
sample_rate = 16000
channels = 1
buffer_size = 1024

[transcription]
model = "nova-3"
language = "en"
smart_format = true
punctuate = true

[ui]
show_tray_icon = true
```

## Development

```bash
just build          # Build project
just run           # Run with release build
just debug         # Run with debug logging
just test          # Run tests
just check         # Format, lint, and test
just watch         # Auto-rebuild on changes
just deepgram-costs # Check API usage
```

## Troubleshooting

### System Tray Icon (GNOME)
GNOME requires AppIndicator extension:

```bash
# Ubuntu/Debian
sudo apt install gnome-shell-extension-appindicator

# Fedora
sudo dnf install gnome-shell-extension-appindicator

# Or install from: https://extensions.gnome.org/extension/615/appindicator-support/
```

Enable in GNOME Extensions app and restart session.

### Common Issues
- **No audio**: Check microphone permissions in system settings
- **Hotkey conflict**: Ensure no other app uses Super+V
- **Config issues**: Check logs with `just debug`

## License

MIT License - see LICENSE file for details
