{
  description = "High-performance Nix module system with Rust primops";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Common build inputs
        commonBuildInputs = with pkgs; [
          nix.dev
          boost
          nlohmann_json
        ];

        commonNativeBuildInputs = with pkgs; [
          pkg-config
          clang
          rustToolchain
        ];

        # Source filtering
        src = pkgs.lib.cleanSourceWith {
          src = craneLib.path ./.;
          filter = path: type:
            (pkgs.lib.hasSuffix ".cpp" path) ||
            (pkgs.lib.hasSuffix ".h" path) ||
            (craneLib.filterCargoSources path type);
        };

        # Common args for crane builds
        commonArgs = {
          inherit src;
          strictDeps = true;

          nativeBuildInputs = commonNativeBuildInputs;
          buildInputs = commonBuildInputs;

          # Required for C++ compilation
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        };

        # Build just the cargo dependencies
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the actual crate
        nix-module-system = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;

          # Additional test dependencies
          nativeCheckInputs = with pkgs; [
            nix
          ];
        });

        # Clippy checks
        nix-module-system-clippy = craneLib.cargoClippy (commonArgs // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        });

        # Format check
        nix-module-system-fmt = craneLib.cargoFmt {
          inherit src;
        };

        # Documentation
        nix-module-system-doc = craneLib.cargoDoc (commonArgs // {
          inherit cargoArtifacts;
        });

      in {
        packages = {
          default = nix-module-system;
          inherit nix-module-system;
          doc = nix-module-system-doc;
        };

        checks = {
          inherit nix-module-system nix-module-system-clippy nix-module-system-fmt;
        };

        devShells.default = craneLib.devShell {
          inputsFrom = [ nix-module-system ];

          packages = with pkgs; [
            # Rust tools
            rust-analyzer
            cargo-watch
            cargo-expand
            cargo-flamegraph

            # Nix tools
            nil
            nixpkgs-fmt

            # Debug tools
            gdb
            lldb
            valgrind
          ];

          # Shell hook for development
          shellHook = ''
            echo "Nix Module System Development Shell"
            echo "===================================="
            echo ""
            echo "Commands:"
            echo "  cargo build --release  Build the plugin"
            echo "  cargo test             Run tests"
            echo "  cargo clippy           Run linter"
            echo ""
            echo "To test the plugin:"
            echo "  nix eval --plugin-files ./target/release/libnix_module_system.so --expr '...'"
          '';
        };

        # For nix run
        apps.default = {
          type = "app";
          program = "${nix-module-system}/bin/nix-module-system";
        };
      });
}
