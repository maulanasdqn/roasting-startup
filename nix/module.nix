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
        - POSTHOG_KEY (optional, for analytics)
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
        LEPTOS_OUTPUT_NAME = "roasting-startup";
        LEPTOS_SITE_ROOT = "${package}/site";
        LEPTOS_SITE_PKG_DIR = "pkg";
        LEPTOS_SITE_ADDR = "${cfg.host}:${toString cfg.port}";
        LEPTOS_ENV = "production";
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

        # Security hardening (relaxed for Chromium compatibility)
        PrivateTmp = true;
        ProtectSystem = "full";  # Changed from "strict" for Chromium
        ProtectHome = "read-only";  # Chromium may need to read some paths
        ReadWritePaths = [ cfg.dataDir "/tmp" ];

        # Chromium sandbox requirements:
        # - NoNewPrivileges must be false for Chromium's setuid sandbox
        # - Namespaces must be allowed for Chromium's namespace sandbox
        NoNewPrivileges = false;

        # Restrict capabilities (keep minimal)
        CapabilityBoundingSet = [ "CAP_SYS_ADMIN" ];  # Needed for Chromium sandbox
        AmbientCapabilities = "";

        # System call filtering - allow Chromium syscalls
        SystemCallArchitectures = "native";
        # Removed restrictive SystemCallFilter - Chromium needs many syscalls

        # Namespace restrictions - disabled for Chromium
        RestrictNamespaces = false;  # Chromium creates namespaces for sandboxing
        RestrictRealtime = true;
        RestrictSUIDSGID = false;  # Chromium sandbox may need this

        # Additional hardening (kept where compatible)
        LockPersonality = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        # Removed ProtectProc and ProcSubset - Chromium reads /proc

        # Memory protection - disabled for Rust runtime and Chromium JIT
        MemoryDenyWriteExecute = false;

        # Devices - Chromium needs /dev/shm and possibly GPU access
        PrivateDevices = false;

        # Restrict address families
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_UNIX" "AF_NETLINK" ];

        # Hide hostname
        ProtectHostname = true;

        # Resource limits (increased for Chromium)
        MemoryMax = "2G";
        TasksMax = 512;  # Chromium spawns many processes
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

      # Rate limiting zone
      appendHttpConfig = ''
        limit_req_zone $binary_remote_addr zone=roasting_limit:10m rate=10r/s;
      '';

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

            # Re-add security headers (add_header in nested location drops parent headers)
            add_header X-Frame-Options "SAMEORIGIN" always;
            add_header X-Content-Type-Options "nosniff" always;
            add_header X-XSS-Protection "1; mode=block" always;
            add_header Referrer-Policy "strict-origin-when-cross-origin" always;
            add_header Permissions-Policy "geolocation=(), microphone=(), camera=()" always;
          '';
        };
      };
    };

    security.acme = lib.mkIf (cfg.nginx.enableSSL && cfg.acmeEmail != null && cfg.domain != null) {
      acceptTerms = true;
      defaults.email = cfg.acmeEmail;
    };

    networking.firewall = lib.mkIf cfg.openFirewall {
      allowedTCPPorts = [ cfg.port ];
    };
  };
}
