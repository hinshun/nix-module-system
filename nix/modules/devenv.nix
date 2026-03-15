# devenv.nix - Developer environment module
# This demonstrates a devenv-style DSL for development environments

{ config, lib, pkgs, ... }:

let
  cfg = config.devenv;
  inherit (lib) mkIf mkOption mkEnableOption mkMerge mkDefault types;
in
{
  options.devenv = {
    # Languages
    languages = {
      rust = {
        enable = mkEnableOption "Rust language support";
        version = mkOption {
          type = types.str;
          default = "stable";
          description = "Rust toolchain version (stable, beta, nightly, or specific version)";
        };
        components = mkOption {
          type = types.listOf types.str;
          default = [ "rustc" "cargo" "clippy" "rustfmt" ];
          description = "Rust components to include";
        };
      };

      go = {
        enable = mkEnableOption "Go language support";
        version = mkOption {
          type = types.str;
          default = "1.21";
          description = "Go version";
        };
      };

      python = {
        enable = mkEnableOption "Python language support";
        version = mkOption {
          type = types.str;
          default = "3.11";
          description = "Python version";
        };
        venv = {
          enable = mkOption {
            type = types.bool;
            default = true;
            description = "Create a virtual environment";
          };
        };
        packages = mkOption {
          type = types.listOf types.str;
          default = [];
          description = "Python packages to install";
          example = [ "requests" "pytest" ];
        };
      };

      javascript = {
        enable = mkEnableOption "JavaScript/TypeScript support";
        package = mkOption {
          type = types.enum [ "nodejs" "bun" "deno" ];
          default = "nodejs";
          description = "JavaScript runtime to use";
        };
        pnpm.enable = mkEnableOption "pnpm package manager";
        yarn.enable = mkEnableOption "yarn package manager";
      };

      nix = {
        enable = mkEnableOption "Nix language support";
        formatter = mkOption {
          type = types.enum [ "nixfmt" "alejandra" "nixpkgs-fmt" ];
          default = "nixfmt";
          description = "Nix formatter to use";
        };
      };
    };

    # Services
    services = {
      postgres = {
        enable = mkEnableOption "PostgreSQL database";
        port = mkOption {
          type = types.port;
          default = 5432;
          description = "Port for PostgreSQL";
        };
        initialDatabases = mkOption {
          type = types.listOf (types.submodule [{
            options = {
              name = mkOption {
                type = types.str;
                description = "Database name";
              };
              schema = mkOption {
                type = types.nullOr types.path;
                default = null;
                description = "Initial schema file";
              };
            };
          }]);
          default = [];
          description = "Databases to create on first start";
        };
      };

      redis = {
        enable = mkEnableOption "Redis cache server";
        port = mkOption {
          type = types.port;
          default = 6379;
          description = "Port for Redis";
        };
      };

      minio = {
        enable = mkEnableOption "MinIO S3-compatible storage";
        port = mkOption {
          type = types.port;
          default = 9000;
          description = "Port for MinIO";
        };
        consolePort = mkOption {
          type = types.port;
          default = 9001;
          description = "Port for MinIO console";
        };
      };
    };

    # Scripts
    scripts = mkOption {
      type = types.attrsOf (types.submodule [{
        options = {
          exec = mkOption {
            type = types.str;
            description = "Script content to execute";
          };
          description = mkOption {
            type = types.nullOr types.str;
            default = null;
            description = "Script description shown in help";
          };
        };
      }]);
      default = {};
      description = "Custom scripts available in the environment";
      example = {
        build = {
          exec = "cargo build --release";
          description = "Build the project";
        };
        test = {
          exec = "cargo test";
          description = "Run tests";
        };
      };
    };

    # Environment variables
    env = mkOption {
      type = types.attrsOf types.str;
      default = {};
      description = "Environment variables to set";
      example = {
        DATABASE_URL = "postgres://localhost/mydb";
        RUST_LOG = "debug";
      };
    };

    # Pre-commit hooks
    pre-commit = {
      hooks = mkOption {
        type = types.attrsOf (types.submodule [{
          options = {
            enable = mkOption {
              type = types.bool;
              default = true;
              description = "Enable this hook";
            };
            entry = mkOption {
              type = types.str;
              description = "Command to run";
            };
            files = mkOption {
              type = types.str;
              default = "";
              description = "File pattern to match";
            };
            pass_filenames = mkOption {
              type = types.bool;
              default = true;
              description = "Pass matched filenames to command";
            };
          };
        }]);
        default = {};
        description = "Pre-commit hooks configuration";
      };
    };

    # Container configuration
    containers = mkOption {
      type = types.attrsOf (types.submodule [{
        options = {
          image = mkOption {
            type = types.str;
            description = "Container image";
          };
          ports = mkOption {
            type = types.listOf types.str;
            default = [];
            description = "Port mappings";
          };
          environment = mkOption {
            type = types.attrsOf types.str;
            default = {};
            description = "Environment variables";
          };
          volumes = mkOption {
            type = types.listOf types.str;
            default = [];
            description = "Volume mounts";
          };
        };
      }]);
      default = {};
      description = "Additional containers to run";
    };

    # Process composition
    processes = mkOption {
      type = types.attrsOf (types.submodule [{
        options = {
          exec = mkOption {
            type = types.str;
            description = "Command to run";
          };
          process-compose = mkOption {
            type = types.attrsOf types.str;
            default = {};
            description = "Process-compose specific settings";
          };
        };
      }]);
      default = {};
      description = "Long-running processes";
    };
  };

  config = mkMerge [
    # Rust configuration
    (mkIf cfg.languages.rust.enable {
      devenv.env = {
        RUST_BACKTRACE = mkDefault "1";
      };
      devenv.scripts.cargo-watch = mkDefault {
        exec = "cargo watch -x check -x test";
        description = "Watch for changes and run checks";
      };
    })

    # Python configuration
    (mkIf cfg.languages.python.enable {
      devenv.env = lib.optionalAttrs cfg.languages.python.venv.enable {
        VIRTUAL_ENV = ".venv";
        PATH = ".venv/bin:$PATH";
      };
    })

    # PostgreSQL configuration
    (mkIf cfg.services.postgres.enable {
      devenv.env = {
        PGHOST = mkDefault "localhost";
        PGPORT = mkDefault (toString cfg.services.postgres.port);
      };
      devenv.scripts.pg-shell = mkDefault {
        exec = "psql";
        description = "Open PostgreSQL shell";
      };
    })

    # Redis configuration
    (mkIf cfg.services.redis.enable {
      devenv.env = {
        REDIS_URL = mkDefault "redis://localhost:${toString cfg.services.redis.port}";
      };
    })

    # Pre-commit hooks setup
    (mkIf (cfg.pre-commit.hooks != {}) {
      devenv.scripts.pre-commit-install = {
        exec = "pre-commit install";
        description = "Install pre-commit hooks";
      };
    })
  ];
}
