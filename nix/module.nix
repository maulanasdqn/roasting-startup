flake:
{ config, lib, pkgs, ... }:

let
  cfg = config.services.roasting-startup;
  package = if cfg.useLocalLlm
    then flake.packages.${pkgs.system}.local-llm
    else flake.packages.${pkgs.system}.default;
in
{
  options.services.roasting-startup = {
    enable = lib.mkEnableOption "Roasting Startup service";

    useLocalLlm = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Use local LLM (SmolLM2-135M-Instruct) instead of OpenRouter API.
        When enabled, no API key is needed but requires more server resources.
        Model will be downloaded from Hugging Face on first run (~270MB).
        Note: SmolLM2-135M is a small model with limited quality.
        For best results, use OpenRouter with a larger model.
      '';
    };

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
      description = "Environment file containing OPENROUTER_API_KEY (not needed if useLocalLlm is true)";
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
      after = [ "network.target" ];

      environment = {
        LEPTOS_SITE_ROOT = "${package}/site";
        LEPTOS_SITE_ADDR = "${cfg.host}:${toString cfg.port}";
        RUST_LOG = "info";
        # HF cache for model downloads
        HF_HOME = "${cfg.dataDir}/.cache/huggingface";
      } // lib.optionalAttrs cfg.useLocalLlm {
        USE_LOCAL_LLM = "1";
      };

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.dataDir;
        ExecStart = "${package}/bin/roasting-api";
        Restart = "on-failure";
        RestartSec = 5;

        EnvironmentFile = lib.mkIf (cfg.environmentFile != null) cfg.environmentFile;

        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        NoNewPrivileges = true;
        ReadWritePaths = [ cfg.dataDir ];

        CapabilityBoundingSet = "";
        SystemCallFilter = [ "@system-service" "~@privileged" ];
        SystemCallArchitectures = "native";
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        MemoryDenyWriteExecute = false;
      };

      path = [ pkgs.chromium ];
    };

    services.nginx = lib.mkIf (cfg.nginx.enable && cfg.domain != null) {
      enable = true;

      virtualHosts.${cfg.domain} = {
        enableACME = cfg.nginx.enableSSL && cfg.acmeEmail != null;
        forceSSL = cfg.nginx.enableSSL && cfg.acmeEmail != null;

        locations."/" = {
          proxyPass = "http://${cfg.host}:${toString cfg.port}";
          proxyWebsockets = true;
          extraConfig = ''
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
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
