# GNOME Voice Input

A voice input utility for the GNOME desktop that transcribes speech using Deepgram and inserts text into the currently focused field.

## Features

- Real-time speech-to-text transcription using Deepgram API
- Global hotkey support (default: Super+V)
- System tray integration (requires AppIndicator extension on GNOME)
- Automatic text insertion into any focused text field
- Configurable audio settings
- Desktop environment detection with helpful setup instructions

## Prerequisites

### System Dependencies

On Debian/Ubuntu:
```bash
sudo apt install libasound2-dev libxdo-dev
```

On Fedora:
```bash
sudo dnf install alsa-lib-devel libxdo-devel
```

### Deepgram API Key

You'll need a Deepgram API key for speech transcription:
1. Sign up at [Deepgram Console](https://console.deepgram.com/)
2. Create a new API key
3. Add it to your config file

## Installation

1. Clone the repository:
```bash
git clone https://github.com/yourusername/gnome-voice-input.git
cd gnome-voice-input
```

2. Build and install:
```bash
just install
```

3. Initialize configuration:
```bash
just init-config
```

4. Edit the config file and add your Deepgram API key:
```bash
nano ~/.config/gnome-voice-input/config.toml
```

## Usage

1. Start the application:
```bash
gnome-voice-input
```

2. The app will run in the system tray
3. Press Super+V (or your configured hotkey) to start/stop recording
4. Speak into your microphone
5. The transcribed text will be typed into the currently focused text field

## Configuration

The configuration file is located at `~/.config/gnome-voice-input/config.toml`:

```toml
# Deepgram API key
deepgram_api_key = "your-api-key-here"

[hotkey]
# Modifier keys: super, ctrl, alt, shift
modifiers = ["super"]
# Key to press with modifiers
key = "v"

[audio]
# Audio settings
sample_rate = 16000
channels = 1
buffer_size = 1024
```

## Development

### Build
```bash
just build
```

### Run with debug logging
```bash
just debug
```

### Run tests
```bash
just test
```

### Format and lint
```bash
just check
```

## Troubleshooting

### No audio input detected
- Check that your microphone is properly connected
- Verify microphone permissions in GNOME Settings
- Run with debug logging to see detected audio devices

### Hotkey not working
- Ensure no other application is using the same hotkey
- Try running the application with sudo (not recommended for production)
- Check if your desktop environment supports global hotkeys

### System tray icon not appearing (GNOME users)

GNOME removed native system tray support. To see the tray icon, you need to install the AppIndicator extension:

#### Option 1: Install via GNOME Extensions website
1. Visit https://extensions.gnome.org/extension/615/appindicator-support/
2. Click "Install" and follow the prompts
3. Log out and log back in

#### Option 2: Install via package manager
```bash
# Ubuntu/Debian
sudo apt install gnome-shell-extension-appindicator

# Fedora
sudo dnf install gnome-shell-extension-appindicator

# Arch
sudo pacman -S gnome-shell-extension-appindicator
```

After installation:
1. Enable the extension in GNOME Extensions app
2. Log out and log back in
3. The tray icon should now appear in the top panel

**Note:** The app will still work via hotkey (Super+V) even without the tray icon.

### System tray icon not appearing (Other desktops)
- KDE Plasma: Should work out of the box
- XFCE: Should work out of the box
- Ensure `libappindicator` or `libayatana-appindicator` is installed

## License

MIT License - see LICENSE file for details