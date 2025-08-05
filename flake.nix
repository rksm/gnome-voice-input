{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          # Rust toolchain
          nativeBuildInputs = with pkgs; [
            rustc
            cargo
            clippy
            pkg-config
            clang # Required for bindgen
          ];

          # System dependencies
          buildInputs = with pkgs; [
            # Networking
            openssl.dev

            # Audio
            alsa-lib.dev

            # D-Bus for StatusNotifier detection
            dbus.dev

            # X11 keyboard simulation
            xdotool
            xorg.libX11.dev

            # System tray support (ksni crate dependencies)
            libappindicator-gtk3 # Or libayatana-appindicator

            # GTK4 for native dialogs
            gtk4
            glib
            cairo
            pango
            gdk-pixbuf
            graphene
          ];

          # Development tools
          packages = with pkgs; [
            rust-analyzer
            (rustfmt.override { asNightly = true; })
          ];

          # Environment variables
          RUST_BACKTRACE = "1";
          RUST_LOG = "debug";
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

          # Library path for runtime linking
          LD_LIBRARY_PATH = with pkgs; pkgs.lib.makeLibraryPath [
            libappindicator-gtk3
          ];
        };
      }
    );
}
