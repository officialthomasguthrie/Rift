# the file manager, rift-files from the workspace package: its row in the Applications menu, the
# app every folder opens in, and the folders of home it and every other app look for. xdg-open, a
# gtk app that opens a folder and the portal's OpenDirectory all ask for the app of inode/directory,
# which was disk usage analyzer's until files claimed it
{ lib, pkgs, ... }:
let
  # the folders of home, the way xdg-user-dirs names them. glib reads this file, so the file chooser
  # lists the folders, a browser saves downloads in Downloads and the camera saves in Pictures. a
  # desktop of icons, templates and a public folder are not part of rift, so those three are home
  # itself, which is how the file says a folder is not wanted. tmpfiles turns the \n into new lines
  # and writes the file once, when there is none, so the owner's own choice stays
  userDirs = lib.concatStringsSep "\\n" [
    ''XDG_DESKTOP_DIR="$HOME"''
    ''XDG_DOWNLOAD_DIR="$HOME/Downloads"''
    ''XDG_TEMPLATES_DIR="$HOME"''
    ''XDG_PUBLICSHARE_DIR="$HOME"''
    ''XDG_DOCUMENTS_DIR="$HOME/Documents"''
    ''XDG_MUSIC_DIR="$HOME/Music"''
    ''XDG_PICTURES_DIR="$HOME/Pictures"''
    ''XDG_VIDEOS_DIR="$HOME/Videos"''
    ""
  ];
in
{
  environment.systemPackages = [
    (pkgs.makeDesktopItem {
      name = "dev.rift.Files";
      desktopName = "Files";
      comment = "Folders, files and the trash";
      exec = "rift-files %U";
      icon = "folder-symbolic";
      categories = [
        "System"
        "FileManager"
      ];
      mimeTypes = [ "inode/directory" ];
    })
  ];

  xdg.mime.defaultApplications."inode/directory" = "dev.rift.Files.desktop";

  # a folder that is not there is made at every login, the way the image makes the settings of the
  # editor and the terminal
  systemd.user.tmpfiles.rules = [
    "d %h/Documents 0755 - - -"
    "d %h/Downloads 0755 - - -"
    "d %h/Music 0755 - - -"
    "d %h/Pictures 0755 - - -"
    "d %h/Videos 0755 - - -"
    "d %h/.config 0755 - - -"
    "f %h/.config/user-dirs.dirs 0644 - - - ${userDirs}"
  ];
}
