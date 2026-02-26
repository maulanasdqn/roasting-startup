flake:
{ config, lib, pkgs, ... }:

let
  cfg = config.services.roasting-startup;
  package = flake.packages.${pkgs.system}.default;
in
{
  options.services.roasting-startup = {
    enable = lib.mkEnableOption "Roasting Startup service";

    port = lib.mkOption {
      type = lib.types.port;
      default = 3000;
      description = "Port to listen on";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "Host to bind to";
    };

    environmentFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        Environment file containing secrets:
        - DATABASE_URL
        - OPENROUTER_API_KEY
        - GOOGLE_CLIENT_ID
        - GOOGLE_CLIENT_SECRET
        - GOOGLE_REDIRECT_URI
      '';
    };

    domain = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Domain for nginx virtual host";
    };

    acmeEmail = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Email for ACME/Let's Encrypt";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "roasting";
      description = "User to run the service as";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "roasting";
      description = "Group to run the service as";
    };

    dataDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/roasting-startup";
      description = "Data directory for the service";
    };

    nginx = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Enable nginx reverse proxy";
      };

      enableSSL = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Enable SSL/TLS with ACME";
      };
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open firewall for direct access (not recommended with nginx)";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.environmentFile != null;
        message = "roasting-startup requires an environment file with DATABASE_URL and other secrets";
      }
    ];

    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      home = cfg.dataDir;
      createHome = true;
    };

    users.groups.${cfg.group} = { };

    systemd.services.roasting-startup = {
      description = "Roasting Startup - Indonesian startup roasting website";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" "postgresql.service" ];
      requires = [ "postgresql.service" ];

      environment = {
        LEPTOS_SITE_ROOT = "${package}/site";
        LEPTOS_SITE_ADDR = "${cfg.host}:${toString cfg.port}";
        RUST_LOG = "info,roasting_app=debug,roasting_api=debug";
      };

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.dataDir;
        ExecStart = "${package}/bin/roasting-api";
        Restart = "on-failure";
        RestartSec = 5;

        EnvironmentFile = cfg.environmentFile;

        # Security hardening
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        NoNewPrivileges = true;
        ReadWritePaths = [ cfg.dataDir ];

        # Restrict capabilities
        CapabilityBoundingSet = "";
        AmbientCapabilities = "";

        # System call filtering
        SystemCallFilter = [ "@system-service" "~@privileged" "~@resources" ];
        SystemCallArchitectures = "native";

        # Namespace restrictions
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;

        # Additional hardening
        LockPersonality = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProcSubset = "pid";

        # Memory protection - disabled for Rust runtime
        MemoryDenyWriteExecute = false;

        # Private devices
        PrivateDevices = true;

        # Remove all capabilities
        SecureBits = "no-setuid-fixup-locked noroot-locked";

        # Restrict address families to only what's needed
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_UNIX" ];

        # Hide /proc entries
        ProtectHostname = true;

        # Limit resource usage
        MemoryMax = "512M";
        TasksMax = 100;
      };

      # Chromium needed for website scraping
      path = [ pkgs.chromium ];
    };

    services.nginx = lib.mkIf (cfg.nginx.enable && cfg.domain != null) {
      enable = true;

      # Security headers
      recommendedGzipSettings = true;
      recommendedOptimisation = true;
      recommendedProxySettings = true;
      recommendedTlsSettings = true;

      virtualHosts.${cfg.domain} = {
        enableACME = cfg.nginx.enableSSL && cfg.acmeEmail != null;
        forceSSL = cfg.nginx.enableSSL && cfg.acmeEmail != null;

        # Security headers
        extraConfig = ''
          add_header X-Frame-Options "SAMEORIGIN" always;
          add_header X-Content-Type-Options "nosniff" always;
          add_header X-XSS-Protection "1; mode=block" always;
          add_header Referrer-Policy "strict-origin-when-cross-origin" always;
          add_header Permissions-Policy "geolocation=(), microphone=(), camera=()" always;
        '';

        locations."/" = {
          proxyPass = "http://${cfg.host}:${toString cfg.port}";
          proxyWebsockets = true;
          extraConfig = ''
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            # Rate limiting at nginx level
            limit_req zone=roasting_limit burst=10 nodelay;
            limit_req_status 429;
          '';
        };

        # Static assets caching
        locations."~* \\.(js|css|png|jpg|jpeg|gif|ico|wasm)$" = {
          proxyPass = "http://${cfg.host}:${toString cfg.port}";
          extraConfig = ''
            proxy_set_header Host $host;
            expires 7d;
            add_header Cache-Control "public, immutable";
          '';
        };
      };
    };

    # Rate limiting zone
    services.nginx.appendHttpConfig = lib.mkIf (cfg.nginx.enable && cfg.domain != null) ''
      limit_req_zone $binary_remote_addr zone=roasting_limit:10m rate=10r/s;
    '';

    security.acme = lib.mkIf (cfg.nginx.enableSSL && cfg.acmeEmail != null && cfg.domain != null) {
      acceptTerms = true;
      defaults.email = cfg.acmeEmail;
    };

    networking.firewall = lib.mkIf cfg.openFirewall {
      allowedTCPPorts = [ cfg.port ];
    };
  };
}
