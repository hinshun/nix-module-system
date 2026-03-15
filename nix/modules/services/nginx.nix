# services.nginx module
# Example of a service module with realistic options

{ config, lib, pkgs, ... }:

let
  cfg = config.services.nginx;
  inherit (lib) mkIf mkOption mkEnableOption mkMerge types;
in
{
  options.services.nginx = {
    enable = mkEnableOption "the nginx web server";

    package = mkOption {
      type = types.package;
      default = pkgs.nginx or null;
      description = "The nginx package to use.";
    };

    user = mkOption {
      type = types.str;
      default = "nginx";
      description = "User account under which nginx runs.";
    };

    group = mkOption {
      type = types.str;
      default = "nginx";
      description = "Group under which nginx runs.";
    };

    httpConfig = mkOption {
      type = types.lines;
      default = "";
      description = "Additional http block configuration.";
    };

    virtualHosts = mkOption {
      type = types.attrsOf (types.submodule [
        ({ name, ... }: {
          options = {
            serverName = mkOption {
              type = types.nullOr types.str;
              default = name;
              description = "Server name for this virtual host.";
            };

            root = mkOption {
              type = types.nullOr types.path;
              default = null;
              description = "Root directory for this virtual host.";
            };

            listen = mkOption {
              type = types.listOf (types.submodule [{
                options = {
                  addr = mkOption {
                    type = types.str;
                    default = "0.0.0.0";
                    description = "Address to listen on.";
                  };
                  port = mkOption {
                    type = types.port;
                    default = 80;
                    description = "Port to listen on.";
                  };
                  ssl = mkOption {
                    type = types.bool;
                    default = false;
                    description = "Enable SSL.";
                  };
                };
              }]);
              default = [{ addr = "0.0.0.0"; port = 80; }];
              description = "Listen addresses and ports.";
            };

            locations = mkOption {
              type = types.attrsOf (types.submodule [{
                options = {
                  proxyPass = mkOption {
                    type = types.nullOr types.str;
                    default = null;
                    description = "Proxy pass URL.";
                  };
                  root = mkOption {
                    type = types.nullOr types.path;
                    default = null;
                    description = "Root directory for this location.";
                  };
                  index = mkOption {
                    type = types.nullOr types.str;
                    default = null;
                    description = "Index file.";
                  };
                  extraConfig = mkOption {
                    type = types.lines;
                    default = "";
                    description = "Extra location configuration.";
                  };
                };
              }]);
              default = {};
              description = "Location blocks for this virtual host.";
            };

            extraConfig = mkOption {
              type = types.lines;
              default = "";
              description = "Extra configuration for this virtual host.";
            };
          };
        })
      ]);
      default = {};
      description = "Virtual host configurations.";
      example = {
        "example.com" = {
          root = "/var/www/example.com";
          locations."/" = {
            index = "index.html";
          };
        };
      };
    };

    recommendedOptimisation = mkOption {
      type = types.bool;
      default = true;
      description = "Enable recommended optimization settings.";
    };

    recommendedTlsSettings = mkOption {
      type = types.bool;
      default = true;
      description = "Enable recommended TLS settings.";
    };

    recommendedGzipSettings = mkOption {
      type = types.bool;
      default = true;
      description = "Enable recommended gzip compression.";
    };
  };

  config = mkIf cfg.enable (mkMerge [
    # Base configuration
    {
      users.users.${cfg.user} = {
        group = cfg.group;
        isSystemUser = true;
      };
      users.groups.${cfg.group} = {};

      systemd.services.nginx = {
        description = "Nginx Web Server";
        after = [ "network.target" ];
        wantedBy = [ "multi-user.target" ];
        serviceConfig = {
          ExecStart = "${cfg.package}/bin/nginx";
          ExecReload = "${cfg.package}/bin/nginx -s reload";
          User = cfg.user;
          Group = cfg.group;
        };
      };
    }

    # Optimization settings
    (mkIf cfg.recommendedOptimisation {
      services.nginx.httpConfig = ''
        sendfile on;
        tcp_nopush on;
        tcp_nodelay on;
        keepalive_timeout 65;
        types_hash_max_size 2048;
      '';
    })

    # Gzip settings
    (mkIf cfg.recommendedGzipSettings {
      services.nginx.httpConfig = ''
        gzip on;
        gzip_vary on;
        gzip_proxied any;
        gzip_comp_level 6;
        gzip_types text/plain text/css text/xml application/json application/javascript;
      '';
    })

    # TLS settings
    (mkIf cfg.recommendedTlsSettings {
      services.nginx.httpConfig = ''
        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_prefer_server_ciphers on;
        ssl_session_cache shared:SSL:10m;
        ssl_session_timeout 1d;
      '';
    })
  ]);
}
