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
            (pkgs.lib.hasSuffix ".nix" path && pkgs.lib.hasInfix "/nix/" path) ||
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

        # Build the plugin (cdylib for nix eval --plugin-files)
        nix-module-plugin = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "nix-module-plugin";
          cargoExtraArgs = "-p nix-module-plugin";

          # Install the shared library to lib/
          postInstall = ''
            mkdir -p $out/lib
            find $out -name 'libnix_module_plugin.so' -o -name 'libnix_module_plugin.dylib' | \
              while read f; do cp "$f" $out/lib/; done
          '';
        });

        # Build the CLI (no longer depends on nix-bindings-rust)
        nix-module-cli = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "nix-module-cli";
          cargoExtraArgs = "-p nix-module-cli";

          nativeCheckInputs = with pkgs; [
            nix
          ];

          # Install nix/lib.nix alongside the binary
          postInstall = ''
            mkdir -p $out/share/nix-module-system/nix
            cp ${./nix/lib.nix} $out/share/nix-module-system/nix/lib.nix
          '';
        });

        # Wrapped CLI that knows where lib.nix and the plugin are
        nix-module = pkgs.writeShellScriptBin "nix-module" ''
          export NMS_LIB_PATH="''${NMS_LIB_PATH:-${nix-module-cli}/share/nix-module-system/nix/lib.nix}"
          export NMS_PLUGIN_PATH="''${NMS_PLUGIN_PATH:-${nix-module-plugin}/lib/libnix_module_plugin.so}"
          exec ${nix-module-cli}/bin/nix-module "$@"
        '';

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
          default = nix-module;
          cli = nix-module-cli;
          plugin = nix-module-plugin;
          wrapped = nix-module;
          doc = nix-module-system-doc;
        };

        checks = {
          inherit nix-module-cli nix-module-plugin nix-module-system-clippy nix-module-system-fmt;
        };

        devShells.default = craneLib.devShell {
          inputsFrom = [ nix-module-cli nix-module-plugin ];

          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

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
            echo "Crates:"
            echo "  nix-module-system   Core library (merge engine, types)"
            echo "  nix-module-cli      CLI (drives nix eval subprocess)"
            echo "  nix-module-plugin   Nix plugin (Nix loads Rust cdylib)"
            echo ""
            echo "Commands:"
            echo "  cargo build -p nix-module-cli      Build the CLI"
            echo "  cargo build -p nix-module-plugin    Build the plugin"
            echo "  cargo test                          Run all tests"
            echo ""
            echo "Evaluate with plugin:"
            echo "  nix eval --plugin-files ./target/release/libnix_module_plugin.so \\"
            echo "    --impure --json --expr 'let lib = import ./nix/lib.nix; in lib.evalModules { modules = [ ./nix/examples/nginx-example.nix ]; }'"
            echo ""
            echo "Evaluate without plugin (pure Nix fallback):"
            echo "  nix eval --impure --json --expr 'let lib = import ./nix/lib.nix; in lib.evalModules { modules = [ ./nix/examples/nginx-example.nix ]; }'"
          '';
        };

        # For nix run
        apps.default = {
          type = "app";
          program = "${nix-module}/bin/nix-module";
        };
      });
}
