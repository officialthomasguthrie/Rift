# the everyday apps a desktop is expected to have: pictures, documents, video, sound, a calculator,
# archives, the disks, where the space went, the characters, the camera and the scanner. gnome's
# own, gtk 4 and libadwaita, so they take the dark and light theme from dconf the way the rest of
# the session does. the browser, the editors, the terminal and the password manager are in apps.nix.
# the hardware a desktop talks to is here too: printers, scanners and the firmware of the machine,
# and so are the screen recorder, the screen reader, the on-screen keyboard and the guide
{
  config,
  lib,
  pkgs,
  ...
}:
let
  # the voice the screen reader speaks with. mbrola is 645 MiB of voice data for voices espeak only
  # uses when it is asked for one of them by name, and the voices it speaks with by default are its
  # own, so it is built without them
  voice = pkgs.espeak-ng.override { mbrolaSupport = false; };

  # a row in the Applications menu for something with no entry of its own
  row =
    {
      id,
      name,
      what,
      exec,
      icon,
      categories,
    }:
    pkgs.makeDesktopItem {
      name = id;
      desktopName = name;
      comment = what;
      inherit exec icon categories;
    };
in
{
  environment.systemPackages = with pkgs; [
    # pictures. loupe reads a file in a sandbox of its own, one process per format
    loupe
    # pdf, djvu and comic books
    papers
    # video and sound, both on gstreamer, which decodes what the image can play
    showtime
    decibels
    gnome-calculator
    # archives. file roller reads and writes tar, zip and the rest through libarchive and calls
    # these for the formats it hands over
    file-roller
    unzip
    zip
    p7zip
    # where the space went
    baobab
    # every character and every emoji, by name
    gnome-characters
    # the camera, and the scanner, which reads a page over sane and writes it as a pdf or a picture
    snapshot
    simple-scan
    # xdg-open and xdg-mime, which apps call to hand a file or a link to the app that owns it
    xdg-utils
    # the screen recorder the key runs. it reads the screen over wlr-screencopy, the protocol the
    # screenshot key uses, and encodes with ffmpeg on the processor, so a machine with no video
    # encoder of its own records all the same
    wf-recorder
    # ffmpeg itself, to cut a recording or turn it into another format. its libraries were in the
    # image already, so this is the commands and nothing more
    ffmpeg
    # the screen reader, the voice it speaks with and the on-screen keyboard. lens turns each of
    # them on and off, from a key and from the row below
    orca
    voice
    wvkbd
    (row {
      id = "dev.rift.ScreenReader";
      name = "Screen reader";
      what = "Read the screen aloud";
      exec = "lens --screen-reader";
      icon = "orca";
      categories = [
        "Utility"
        "Accessibility"
      ];
    })
    (row {
      id = "dev.rift.Keyboard";
      name = "On-screen keyboard";
      what = "Type with the pointer or a touch screen";
      exec = "lens --keyboard";
      icon = "input-keyboard";
      categories = [
        "Utility"
        "Accessibility"
      ];
    })
    (row {
      id = "dev.rift.Guide";
      name = "Rift guide";
      what = "How Rift works, on the drive itself";
      exec = "rift guide";
      icon = "help-browser-symbolic";
      categories = [
        "System"
        "Documentation"
      ];
    })
  ];

  # the accessibility bus the screen reader reads the session through: at-spi2-core's registry,
  # which the session bus starts when something asks for it. without it nixos sets NO_AT_BRIDGE and
  # GTK_A11Y=none for every app, and no app says anything about what it is showing
  services.gnome.at-spi2-core.enable = true;

  # the speech the screen reader asks for. speech-dispatcher takes the words and hands them to
  # espeak, which is a voice of a few megabytes rather than a recorded one, so it is on the drive
  # and needs no network. it starts from its own socket in the session, and only when something
  # speaks
  services.speechd = {
    enable = true;
    package = pkgs.speechd.override { espeak = voice; };
  };

  # the guide, as pages on the drive itself. `rift guide` and the row in the Applications menu open
  # them in the browser, so a person reads them on a machine with no network
  environment.etc."rift/guide".source = ../guide;

  # the disks, their partitions and their smart counters. gnome disks asks udisks over the system
  # bus, which starts when it does
  programs.gnome-disks.enable = true;
  services.udisks2.enable = true;

  # printing. cups with its filters, and no driver from any printer maker: a driverless printer
  # says over ipp what it takes (ipp everywhere, which apple calls airprint) and takes pdf or
  # raster, which the filters make from what the app printed. a printer that needs a binary of its
  # own is not one rift prints to. cupsd listens on the local socket and on localhost, and shares
  # nothing
  services.printing.enable = true;

  # mdns, which is how a printer or a scanner on the network is found. rift asks and says nothing
  # about itself: publishing is off, so a borrowed network never hears the machine announce itself,
  # and wide-area discovery is off, so the questions stay on the link. cups-browsed comes with
  # avahi and makes a queue for each driverless printer it hears
  services.avahi = {
    enable = true;
    nssmdns4 = true;
    wideArea = false;
  };

  # scanners, over usb and over the network. sane-airscan is the driverless one, escl and wsd,
  # which is the scanning half of what a driverless printer does. sane's own backends were in the
  # image already, since colord carries them for its scanner helper, and the udev rules tag a
  # scanner with uaccess, so the owner reaches it without being put in a group
  hardware.sane = {
    enable = true;
    extraBackends = [ pkgs.sane-airscan ];
  };

  # the firmware of the machine: every device that has one, its version, and the security
  # attributes that say whether secure boot, the tpm and the iommu are on. it is read and never
  # written, see the polkit rule below
  services.fwupd.enable = true;

  # nothing to refresh when nothing is ever installed, so the daily download of the vendor
  # metadata is off and the remote it comes from is disabled. a machine that may be someone else's
  # does not call a vendor once a day either
  environment.etc."fwupd/remotes.d/lvfs.conf".source = lib.mkForce (
    pkgs.runCommand "lvfs-disabled.conf" { } ''
      sed 's,^Enabled=true,Enabled=false,' \
        ${config.services.fwupd.package}/etc/fwupd/remotes.d/lvfs.conf > "$out"
    ''
  );
  systemd.timers.fwupd-refresh.wantedBy = lib.mkForce [ ];

  security.polkit.extraConfig = lib.mkMerge [
    # udisks answers questions, and asks polkit before it writes anything, so the disk utility
    # shows every disk and its counters while the drive the machine boots from stays as it is. the
    # ids that end in -system are the ones udisks asks for when the device is internal to the
    # machine: mounting, unlocking, formatting, partitioning, opening the raw device, standby and
    # eject. swap is refused with them, and so are the three that erase a whole drive, which have
    # no id of their own for an internal device; formatting a removable disk is another action, and
    # the owner keeps that. a host disk is still mounted read-only through orbit, and nowhere else
    ''
      polkit.addRule(function (action, subject) {
        var id = action.id;
        if (id.indexOf("org.freedesktop.udisks2.") != 0) {
          return;
        }
        if (id.lastIndexOf("-system") == id.length - 7
            || id == "org.freedesktop.udisks2.manage-swapspace"
            || id == "org.freedesktop.udisks2.ata-secure-erase"
            || id == "org.freedesktop.udisks2.nvme-sanitize"
            || id == "org.freedesktop.udisks2.nvme-format-namespace") {
          return polkit.Result.NO;
        }
      });
    ''
    # the same shape for fwupd. the machine rift boots is often not the owner's, and the partition
    # a firmware is staged on would be the drive's own, so the next machine the drive is plugged
    # into is the one that would be written to. every action that writes is refused: the updates,
    # the downgrades, unlocking and activating a device, the daemon's settings, its remotes, the
    # bios settings, the stored checksums and the emulation. the three that only read are left as
    # they are, and reading a device or the security attributes asks polkit nothing at all. fwupd
    # ships a rule of its own that gives an active user in wheel an update with no password, and
    # this file is read before it
    ''
      polkit.addRule(function (action, subject) {
        var id = action.id;
        if (id.indexOf("org.freedesktop.fwupd.") != 0) {
          return;
        }
        if (id == "org.freedesktop.fwupd.get-remotes"
            || id == "org.freedesktop.fwupd.get-bios-settings"
            || id == "org.freedesktop.fwupd.verify") {
          return;
        }
        return polkit.Result.NO;
      });
    ''
  ];

  # the emoji the character picker shows, and the ones every other app draws. the image had letters
  # alone until now, so an emoji in a page or a message was an empty box
  fonts.packages = [ pkgs.noto-fonts-color-emoji ];

  # what opens a file the owner picks in another app
  xdg.mime.defaultApplications =
    let
      pictures = [
        "image/jpeg"
        "image/png"
        "image/gif"
        "image/webp"
        "image/tiff"
        "image/bmp"
        "image/avif"
        "image/heic"
        "image/jxl"
        "image/svg+xml"
        "image/vnd.microsoft.icon"
      ];
      documents = [
        "application/pdf"
        "image/vnd.djvu"
        "application/vnd.comicbook+zip"
        "application/x-cbz"
        "application/x-cbr"
      ];
      video = [
        "video/mp4"
        "video/x-matroska"
        "video/webm"
        "video/quicktime"
        "video/mpeg"
        "video/x-msvideo"
        "video/ogg"
      ];
      sound = [
        "audio/mpeg"
        "audio/flac"
        "audio/x-vorbis+ogg"
        "audio/ogg"
        "audio/x-wav"
        "audio/mp4"
        "audio/x-opus+ogg"
        "audio/x-m4b"
      ];
      archives = [
        "application/zip"
        "application/x-tar"
        "application/x-compressed-tar"
        "application/gzip"
        "application/x-xz"
        "application/zstd"
        "application/x-bzip2"
        "application/x-7z-compressed"
      ];
      # a page, and a link an app hands over. the guide is a page like any other, so this is what
      # opens it
      web = [
        "text/html"
        "application/xhtml+xml"
        "x-scheme-handler/http"
        "x-scheme-handler/https"
      ];
      opens = app: types: builtins.listToAttrs (map (type: lib.nameValuePair type app) types);
    in
    opens "org.gnome.Loupe.desktop" pictures
    // opens "org.gnome.Papers.desktop" documents
    // opens "org.gnome.Showtime.desktop" video
    // opens "org.gnome.Decibels.desktop" sound
    // opens "org.gnome.FileRoller.desktop" archives
    // opens "firefox.desktop" web;
}
