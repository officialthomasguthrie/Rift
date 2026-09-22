# the second tier of apps: the ones rift suggests and installs only when the owner ticks them in
# welcome. flathub is a remote of the system installation, the drive's @flatpak volume, from the
# start: flatpak reads the file in /etc/flatpak/remotes.d the first time anything uses that
# installation, with no network, and flatpak's own polkit rule allows the owner, who is in wheel,
# to install from it without a password. welcome starts with every session and goes away at once on a
# drive that has been welcomed
{
  config,
  pkgs,
  self,
  ...
}:
let
  welcome = "${self.packages.${pkgs.stdenv.hostPlatform.system}.workspace}/bin/rift-welcome";
in
{
  # flathub's own file, as dl.flathub.org publishes it, with the key its summary is signed with
  environment.etc."flatpak/remotes.d/flathub.flatpakrepo".source = ../welcome/flathub.flatpakrepo;
  # the apps welcome lists, one file for it and for the store later
  environment.etc."rift/apps.toml".source = ../welcome/apps.toml;
  # its log goes to the journal under welcome
  rift.horizon.startup = [
    [
      "${config.systemd.package}/bin/systemd-cat"
      "-t"
      "welcome"
      welcome
      "--login"
    ]
  ];
  environment.systemPackages = [
    (pkgs.makeDesktopItem {
      name = "dev.rift.Welcome";
      desktopName = "Welcome";
      comment = "How the desktop looks, apps from Flathub and more languages";
      exec = "rift-welcome";
      icon = "emoji-flags-symbolic";
      categories = [ "System" ];
    })
  ];
}
