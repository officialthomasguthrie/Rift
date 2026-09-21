# what every rift image has
{
  config,
  lib,
  pkgs,
  self,
  ...
}:
let
  system = pkgs.stdenv.hostPlatform.system;
  riftWorkspace = self.packages.${system}.workspace;
  # the folder on persist that holds the link to the time zone the owner chose, and the link
  zoneFolder = "/var/lib/rift/zone";
  zoneLink = "${zoneFolder}/localtime";
in
{
  networking.hostName = "rift";
  networking.networkmanager = {
    enable = true;
    wifi.backend = "iwd";
  };
  networking.nftables.enable = true;
  networking.firewall.enable = true;

  # the time zone is the owner's, chosen on the Date and time page or with timedatectl. the image is
  # read only and root is a tmpfs, so timedated keeps its link on persist and /etc/localtime points
  # at that link, the way the machine id is kept. pid 1 watches the same link, so a timer on the
  # calendar follows a change. a drive where no zone was ever chosen has no link, which glibc and
  # timedated both read as UTC
  time.timeZone = null;
  environment.etc.localtime = {
    source = zoneLink;
    mode = "direct-symlink";
  };
  systemd.services.systemd-timedated.environment.SYSTEMD_ETC_LOCALTIME = zoneLink;
  systemd.managerEnvironment.SYSTEMD_ETC_LOCALTIME = zoneLink;
  # timedated runs with the whole system read only but /etc, so the folder of its own is the one
  # more place it may write. a new persist is made with the folder, since pid 1 watches it from its
  # first moment, and a drive made before it gets it here
  systemd.services.systemd-timedated.serviceConfig.ReadWritePaths = [ "-${zoneFolder}" ];
  systemd.tmpfiles.rules = [ "d ${zoneFolder} 0755 root root -" ];
  # timedated asks for an administrator's password before it changes the zone, and nothing in the
  # session can answer that. the owner may change it from their own session without one, which is
  # what GNOME's own rule gives an administrator for the clock
  security.polkit.extraConfig = ''
    polkit.addRule(function (action, subject) {
      if (action.id == "org.freedesktop.timedate1.set-timezone"
          && subject.local && subject.active && subject.isInGroup("wheel")) {
        return polkit.Result.YES;
      }
    });
  '';

  # british english, which is what rift's own words are written in: day before month, a 24 hour
  # clock, weeks that start on monday, a4 paper and metric measures. en_US stays installed beside it
  i18n.defaultLocale = "en_GB.UTF-8";

  services.pipewire = {
    enable = true;
    alsa.enable = true;
    pulse.enable = true;
  };
  security.rtkit.enable = true;
  hardware.bluetooth.enable = true;
  # the battery and the charger: the bar's battery icon asks upower, and the system menu will ask
  # it how long is left
  services.upower.enable = true;

  # its greeting is in identity.nix
  programs.fish.enable = true;
  documentation.man.enable = true; # quasar indexes man pages offline

  # dev account until first boot setup replaces it with the real owner
  users.mutableUsers = false;
  users.users.rift = {
    isNormalUser = true;
    description = "Rift owner";
    extraGroups = [
      "wheel"
      "networkmanager"
      "video"
      "input"
      "render"
    ];
    shell = pkgs.fish;
    initialPassword = "rift";
  };
  security.sudo.wheelNeedsPassword = false;

  environment.systemPackages = with pkgs; [
    riftWorkspace # quasard, orbit, lens, rift, ...
    helix
    zellij
    nushell
    git
    curl
    ripgrep
    fd
    btop
    pciutils
    usbutils
    dmidecode
    btrfs-progs
    cryptsetup
    gptfdisk
  ];

  system.stateVersion = "26.05";
}
