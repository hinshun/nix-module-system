# Example devenv configuration
# This demonstrates the devenv-style DSL

{ config, lib, pkgs, ... }:

{
  imports = [
    ../modules/devenv.nix
  ];

  # Development environment for a Rust web service
  devenv = {
    # Language configuration
    languages = {
      rust = {
        enable = true;
        version = "stable";
        components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" ];
      };

      nix = {
        enable = true;
        formatter = "alejandra";
      };
    };

    # Services
    services = {
      postgres = {
        enable = true;
        port = 5432;
        initialDatabases = [
          { name = "myapp_dev"; }
          { name = "myapp_test"; }
        ];
      };

      redis = {
        enable = true;
        port = 6379;
      };
    };

    # Custom scripts
    scripts = {
      dev = {
        exec = ''
          echo "Starting development server..."
          cargo watch -x run
        '';
        description = "Start development server with auto-reload";
      };

      test = {
        exec = "cargo test -- --test-threads=1";
        description = "Run tests sequentially";
      };

      lint = {
        exec = ''
          cargo clippy -- -D warnings
          cargo fmt --check
        '';
        description = "Run linting checks";
      };

      db-reset = {
        exec = ''
          dropdb --if-exists myapp_dev
          createdb myapp_dev
          cargo run --bin migrate
        '';
        description = "Reset development database";
      };

      release = {
        exec = ''
          cargo build --release
          echo "Built: target/release/myapp"
        '';
        description = "Build release binary";
      };
    };

    # Environment variables
    env = {
      DATABASE_URL = "postgres://localhost/myapp_dev";
      REDIS_URL = "redis://localhost:6379";
      RUST_LOG = "debug,hyper=info";
      RUST_BACKTRACE = "1";
    };

    # Pre-commit hooks
    pre-commit.hooks = {
      rustfmt = {
        entry = "cargo fmt --";
        files = "\\.rs$";
      };
      clippy = {
        entry = "cargo clippy --";
        files = "\\.rs$";
        pass_filenames = false;
      };
      nixfmt = {
        entry = "alejandra";
        files = "\\.nix$";
      };
    };

    # Background processes
    processes = {
      api = {
        exec = "cargo run --bin api";
        process-compose = {
          depends_on = "postgres,redis";
        };
      };
      worker = {
        exec = "cargo run --bin worker";
        process-compose = {
          depends_on = "redis";
        };
      };
    };
  };
}
