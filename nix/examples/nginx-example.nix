# Example nginx configuration
# This demonstrates the services.nginx module

{ config, lib, pkgs, ... }:

{
  imports = [
    ../modules/services/nginx.nix
  ];

  services.nginx = {
    enable = true;

    recommendedOptimisation = true;
    recommendedGzipSettings = true;
    recommendedTlsSettings = true;

    virtualHosts = {
      "example.com" = {
        root = "/var/www/example.com";
        listen = [
          { addr = "0.0.0.0"; port = 80; }
          { addr = "0.0.0.0"; port = 443; ssl = true; }
        ];

        locations = {
          "/" = {
            index = "index.html index.htm";
          };

          "/api" = {
            proxyPass = "http://localhost:8080";
            extraConfig = ''
              proxy_set_header Host $host;
              proxy_set_header X-Real-IP $remote_addr;
            '';
          };

          "~ \\.php$" = {
            extraConfig = ''
              fastcgi_pass unix:/run/php-fpm/www.sock;
              fastcgi_index index.php;
            '';
          };

          "/static" = {
            root = "/var/www/static";
            extraConfig = ''
              expires 30d;
              add_header Cache-Control "public, immutable";
            '';
          };
        };

        extraConfig = ''
          error_page 404 /404.html;
          error_page 500 502 503 504 /50x.html;
        '';
      };

      "api.example.com" = {
        listen = [{ addr = "0.0.0.0"; port = 443; ssl = true; }];

        locations."/" = {
          proxyPass = "http://localhost:3000";
          extraConfig = ''
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
          '';
        };
      };

      # Default server for unmatched hosts
      "_" = {
        listen = [{ addr = "0.0.0.0"; port = 80; }];
        extraConfig = ''
          return 444;
        '';
      };
    };

    httpConfig = ''
      # Custom logging format
      log_format main '$remote_addr - $remote_user [$time_local] "$request" '
                      '$status $body_bytes_sent "$http_referer" '
                      '"$http_user_agent" "$http_x_forwarded_for"';
      access_log /var/log/nginx/access.log main;

      # Rate limiting
      limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
    '';
  };
}
