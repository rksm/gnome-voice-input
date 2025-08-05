{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default;

          gnome-voice-input = pkgs.rustPlatform.buildRustPackage {
            pname = "gnome-voice-input";
            version = "0.1.0";

            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            nativeBuildInputs = with pkgs; [
              pkg-config
              rustToolchain
            ];

            buildInputs = with pkgs; [
              openssl
              alsa-lib
              dbus
              xorg.libX11
              xorg.libXtst
              xorg.libXi
              libappindicator-gtk3
            ];

            # Set environment variables for the build
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

            meta = with pkgs.lib; {
              description = "Voice input utility for GNOME desktop using Deepgram";
              homepage = "https://github.com/yourusername/gnome-voice-input";
              license = licenses.mit;
              maintainers = [ ];
              platforms = platforms.linux;
            };
          };
        in
        {
          packages = {
            default = gnome-voice-input;
            gnome-voice-input = gnome-voice-input;
          };

          apps.default = flake-utils.lib.mkApp {
            drv = gnome-voice-input;
          };

          devShells.default = pkgs.mkShell {
            # Inherit from the package derivation
            inputsFrom = [ gnome-voice-input ];

            # Additional development tools not needed for building
            nativeBuildInputs = (gnome-voice-input.nativeBuildInputs or [ ]) ++ (with pkgs; [
              rustc
              cargo
              clippy
              rust-analyzer
              rustfmt
              clang # Required for bindgen
            ]);

            # Development-specific environment variables
            RUST_BACKTRACE = "1";
            RUST_LOG = "debug";
            LIBCLANG_PATH = gnome-voice-input.LIBCLANG_PATH;

            # Library path for runtime linking
            LD_LIBRARY_PATH = with pkgs; pkgs.lib.makeLibraryPath [
              libappindicator-gtk3
            ];
          };
        }
      ) // {
      # Overlay to add the package to nixpkgs
      overlays.default = final: prev: {
        gnome-voice-input = self.packages.${final.system}.gnome-voice-input;
      };
    };
}
