# lens: the shell. the top bar along the top of the screen and the applications menu under it,
# with four interpreters behind its field (launcher, os commands, nushell, quasar). one process
# with a layer surface per part, started with the session and restarted if it crashes.
{
  config,
  lib,
  pkgs,
  self,
  ...
}:
let
  cfg = config.rift.lens;
  lens = "${self.packages.${pkgs.stdenv.hostPlatform.system}.workspace}/bin/lens";
  systemctl = "${config.systemd.package}/bin/systemctl --user";
  # the shell asks date, nmcli, wpctl and upower what to show, and starts apps from their desktop
  # entries. the user manager does not inherit the session's path, so the unit names it
  path = lib.concatStringsSep ":" [
    "/run/wrappers/bin"
    "/etc/profiles/per-user/${config.rift.horizon.user}/bin"
    "/run/current-system/sw/bin"
    "/var/lib/flatpak/exports/bin"
  ];
in
{
  options.rift.lens.enable = lib.mkEnableOption "Lens, the Rift shell";

  config = lib.mkIf cfg.enable {
    # the lens binary comes with the workspace package in profiles/base.nix. it is a layer-shell
    # client, so it starts once the display is up: horizon imports WAYLAND_DISPLAY into the user
    # manager and then starts this unit from its startup list. the unit ends with the session and
    # comes back about a second after a crash, with its log in the journal under lens
    systemd.user.services.lens = {
      description = "Lens, the Rift shell";
      partOf = [ "graphical-session.target" ];
      after = [ "graphical-session.target" ];
      serviceConfig = {
        ExecStart = lens;
        Environment = [ "PATH=${path}" ];
        Restart = "always";
        RestartSec = 1;
      };
    };
    # a session that ended left the unit failed, and systemd refuses to start a unit again while
    # its restart counter is over the limit, so the counter is cleared first
    rift.horizon.startup = [
      [
        pkgs.runtimeShell
        "-c"
        "${systemctl} reset-failed lens.service; exec ${systemctl} start lens.service"
      ]
    ];
    # what the os commands run is already there: nmcli with networkmanager, wpctl with pipewire,
    # brightnessctl with horizon, systemctl always. nushell is the third interpreter: lens runs
    # this binary with an argument vector, and it is the interactive nushell as well
    environment.systemPackages = [ pkgs.nushell ];
  };
}
