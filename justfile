default:
    @just --list

# Build the project
build:
    cargo build

# Build in release mode
release:
    cargo build --release

# Run the application
run:
    RUST_LOG=debug cargo run

# Run tests
test:
    cargo test -- --test-threads=1

# Format code
fmt:
    cargo fmt

# Run lints
lint:
    cargo clippy -- -D warnings

# Clean build artifacts
clean:
    cargo clean

# Install the application
install: release
    cargo install --path .

# Create default config if it doesn't exist
init-config:
    mkdir -p ~/.config/gnome-voice-input
    cp config/default.toml ~/.config/gnome-voice-input/config.toml
    @echo "Config created at ~/.config/gnome-voice-input/config.toml"
    @echo "Please add your Deepgram API key to the config file"

# Check all (format, lint, test)
check: fmt lint test

# Watch for changes and rebuild
watch:
    cargo watch -x run

# Build Debian package (requires cargo-deb)
deb:
    cargo deb

# Show system dependencies
deps:
    @echo "System dependencies required:"
    @echo "- libasound2-dev (Debian/Ubuntu)"
    @echo "- alsa-lib-devel (Fedora)"
    @echo "- libxdo-dev (for X11 support)"
    @echo ""
    @echo "Install on Debian/Ubuntu:"
    @echo "  sudo apt install libasound2-dev libxdo-dev"
    @echo ""
    @echo "Install on Fedora:"
    @echo "  sudo dnf install alsa-lib-devel libxdo-devel"
