# the everyday apps a desktop is expected to have: pictures, documents, video, sound, a calculator,
# archives, the disks, where the space went, and the characters. gnome's own, gtk 4 and libadwaita,
# so they take the dark and light theme from dconf the way the rest of the session does. the
# browser, the editors, the terminal and the password manager are in apps.nix
{ lib, pkgs, ... }:
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
    # xdg-open and xdg-mime, which apps call to hand a file or a link to the app that owns it
    xdg-utils
  ];

  # the disks, their partitions and their smart counters. gnome disks asks udisks over the system
  # bus, which starts when it does. what udisks refuses on a host disk is in airlock.nix
  programs.gnome-disks.enable = true;
  services.udisks2.enable = true;

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
      opens = app: types: builtins.listToAttrs (map (type: lib.nameValuePair type app) types);
    in
    opens "org.gnome.Loupe.desktop" pictures
    // opens "org.gnome.Papers.desktop" documents
    // opens "org.gnome.Showtime.desktop" video
    // opens "org.gnome.Decibels.desktop" sound
    // opens "org.gnome.FileRoller.desktop" archives;
}
