{ config, pkgs, lib, ... }:

let
  cfg = config.services.webrtc;
  inherit (lib) types mkOption mkIf optional escapeShellArg;
  frontend = pkgs.callPackage ./client/webrtc.nix {};
  backend = pkgs.callPackage ./server/webrtc.nix {};
in {
  options.services.webrtc = {
    enable = mkOption {
      type = types.bool;
      default = false;
    };

    virtualHost = mkOption {
      type = types.str;
      default = pkgs.lib.getFqdn config;
    };

    backend = mkOption {
      type = types.submodule {
        options = {
          address = mkOption {
            type = types.str;
          };
        };
      };
    };
  };

  config = mkIf cfg.enable {
    services.nginx = {
      enable = true;
      virtualHosts.${cfg.virtualHost}.locations = {
        "/webrtc/signalling/".extraConfig = ''
          proxy_pass http://${cfg.backend.address}/ ;
          proxy_http_version 1.1 ;
          proxy_set_header Upgrade $http_upgrade ;
          proxy_set_header Connection "upgrade" ;
          proxy_read_timeout 86400 ;
        '';

        "/webrtc/".extraConfig = ''
          alias "${frontend}/" ;
          try_files $uri /index.html =404 ;
        '';
      };
    };

    systemd.services.webrtc = {
      wantedBy = ["multi-user.target"];
      serviceConfig = {
        Type = "simple";
        ExecStart = "${backend}/bin/signalling ${cfg.backend.address}";
      };
    };
  };
}
