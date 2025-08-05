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

### Traditional Installation

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

### Nix Installation

#### Using Nix Flakes

```bash
# Run directly without installation
nix run github:yourusername/gnome-voice-input

# Install to user profile
nix profile install github:yourusername/gnome-voice-input

# Build locally
nix build .#gnome-voice-input
```

#### Home Manager Setup (Recommended)

For user-level management with Home Manager:

```nix
{ config, pkgs, ... }:

{
  # Add the overlay to get the package
  nixpkgs.overlays = [
    (final: prev: {
      gnome-voice-input = (builtins.getFlake "github:yourusername/gnome-voice-input").packages.${pkgs.system}.default;
    })
  ];

  # Install the package
  home.packages = [ pkgs.gnome-voice-input ];

  # Create a user systemd service
  systemd.user.services.gnome-voice-input = {
    Unit = {
      Description = "GNOME Voice Input";
      After = [ "graphical-session.target" ];
      PartOf = [ "graphical-session.target" ];
    };

    Service = {
      Type = "simple";
      ExecStart = "${pkgs.gnome-voice-input}/bin/gnome-voice-input";
      Restart = "on-failure";
      RestartSec = 5;
      
      # Set environment for the API key (secure method)
      # Create this file with: echo "your-actual-api-key" > ~/.config/gnome-voice-input/api-key
      # chmod 600 ~/.config/gnome-voice-input/api-key
      Environment = "DEEPGRAM_API_KEY_FILE=%h/.config/gnome-voice-input/api-key";
    };

    Install = {
      WantedBy = [ "graphical-session.target" ];
    };
  };

  # Create config file (without the API key)
  xdg.configFile."gnome-voice-input/config.toml".text = ''
    # API key is loaded from DEEPGRAM_API_KEY_FILE environment variable
    hotkey = "Super+V"

    [audio]
    sample_rate = 16000
    channels = 1
    buffer_size = 512

    [transcription]
    model = "nova-3"
    language = "en"
    smart_format = true
    punctuate = true

    [ui]
    show_tray_icon = true
    notifications = true
  '';
}
```

#### Setting up the API Key Securely

Store your Deepgram API key in a separate file:

```bash
# Create the config directory if it doesn't exist
mkdir -p ~/.config/gnome-voice-input

# Write your API key to a file (replace with your actual key)
echo "your-actual-deepgram-api-key" > ~/.config/gnome-voice-input/api-key

# Set proper permissions (readable only by you)
chmod 600 ~/.config/gnome-voice-input/api-key
```

Alternatively, you can use Home Manager's `age` module or `sops-nix` for encrypted secrets management.

### Development with Nix

```bash
# Enter development shell
nix develop

# Or run commands directly
nix develop -c just check
nix develop -c just test
```

## Usage

1. Start the application:
```bash
gnome-voice-input

# Or with a custom config file:
gnome-voice-input --config /path/to/config.toml

# Enable debug mode (saves audio chunks as WAV files):
gnome-voice-input --debug
```

2. The app will run in the system tray (if enabled)
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
audio_chunk_ms = 25  # Audio chunk size in milliseconds

[transcription]
# Enable interim results for real-time transcription display
use_interim_results = true
# Deepgram model (nova-3, nova-2, etc.)
model = "nova-3"
# Language code (en, es, fr, de, etc.)
language = "en"
# Enable smart formatting (numbers, dates, times)
smart_format = true
# Enable automatic punctuation
punctuate = true

[ui]
# Show system tray icon
show_tray_icon = true
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
