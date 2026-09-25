# horizon: the compositor. greetd starts it on tty1 as the owner, with a pam session and the user's
# systemd manager, no greeter in between. the serial console keeps its own getty.
{
  config,
  lib,
  pkgs,
  self,
  ...
}:
let
  cfg = config.rift.horizon;
  horizon = self.packages.${pkgs.stdenv.hostPlatform.system}.horizon;
  # the photographs the owner can pick from, each with its text file
  wallpapers = pkgs.callPackage ../wallpapers { };
  # the console: a terminal that drops down from the top of the screen over whatever is open. the
  # bind shows or hides the window with this app id, and starts ghostty with it when there is none
  console = {
    appId = "dev.rift.Console";
    height = 400;
  };
  # the session greetd runs. greetd drops the session's own output, systemd-cat puts horizon's log
  # in the journal
  session = "${config.systemd.package}/bin/systemd-cat -t horizon ${horizon}/bin/horizon --session";
  # the lock screen, from the workspace package. its log goes to the journal under lock
  lock = [
    "${config.systemd.package}/bin/systemd-cat"
    "-t"
    "lock"
    "${self.packages.${pkgs.stdenv.hostPlatform.system}.workspace}/bin/horizon-lock"
  ];
  # ghostty's settings, written into the owner's home once, when there is no file yet. tmpfiles
  # turns the \n into new lines. the background is the near black the logo was drawn on, and the
  # title bar follows the desktop, dark or light. the last line reads the colours settings writes,
  # which ghostty takes after this whole file, so they stand in place of the two above. a file that
  # is not there is no error, which is a drive whose owner has never picked another scheme
  ghosttySettings = lib.concatStringsSep "\\n" [
    "font-family = DejaVu Sans Mono"
    "font-size = 11"
    "background = #040406"
    "foreground = #d4d4d4"
    "window-theme = system"
    "config-file = ?~/.config/rift/terminal.ghostty"
  ];
  # the dark theme for gtk 3. gtk 3 has no colour scheme: it takes the dark stylesheet by the name
  # of its theme, and the name gnome writes for dark is Adwaita-dark. gtk 3 carries that stylesheet
  # as a resource but ships no theme by that name, and a name it cannot find falls back to light
  # Adwaita, which is why the portal's own dialogs came up light over a dark desktop. this is the
  # theme: the one line that pulls the stylesheet in, the same file gnome-themes-extra installs,
  # without the gtk 2 engine that comes with it
  adwaitaDark = pkgs.runCommand "adwaita-dark-gtk3" { } ''
    mkdir -p $out/share/themes/Adwaita-dark/gtk-3.0
    echo '@import url("resource:///org/gtk/libgtk/theme/Adwaita/gtk-contained-dark.css");' \
      > $out/share/themes/Adwaita-dark/gtk-3.0/gtk.css
  '';

  # the part of the config written from the owner's theme and wallpaper: by the shell when the
  # session starts, and by rift wallpaper set. horizon reads its config again when the file changes,
  # and a file that is not there yet is no error
  themePart = "~/.local/state/rift/horizon.kdl";
  # the part written from the host profile orbit keeps: how big each screen is drawn. the shell
  # writes it when the session starts and settings writes it again when the displays page changes
  # a scale, since the profile itself is root's
  screensPart = "~/.local/state/rift/displays.kdl";
  # the part written from the owner's settings for the mouse and the touchpad, by settings when the
  # Mouse and touchpad page changes one and by the shell when the session starts. its blocks take the
  # place of the input blocks below, so the page's defaults are the ones written here
  pointerPart = "~/.local/state/rift/pointer.kdl";
  # the system config. the binary still reads the niri paths: /etc/niri/config.kdl here, and a
  # file at ~/.config/niri/config.kdl replaces it for that user
  configFile = pkgs.writeText "horizon-config.kdl" ''
    // the keyboard's layouts come from localed, which the Keyboard page writes, while xkb is empty.
    // a touchpad clicks with a tap, scrolls the content, and ignores a palm while a key is typed
    input {
        keyboard {
            xkb {
            }
        }
        touchpad {
            tap
            natural-scroll
            dwt
        }
    }

    layout {
        gaps 8
        background-color "${cfg.background}"
        focus-ring {
            width 2
            active-color "#78aeed"
            inactive-color "#505050"
        }
        border {
            off
        }
        default-column-width { proportion 0.5; }
    }

    // the system's photograph, drawn under the windows of every workspace and scaled to fill each
    // output. the background colour above shows while it is read, and when the owner picks a colour
    wallpaper "${cfg.wallpaper}"

    // apps draw their own title bars, with the close button, the way gtk and firefox do on gnome.
    // every window is told it is tiled, so it draws square corners and no shadow of its own and
    // sits flush inside the focus ring. a window opens as a column of the default width even when
    // it asks to open maximized, as firefox does on a small screen; maximizing it later still works
    window-rule {
        tiled-state true
        open-maximized-to-edges false
    }

    cursor {
        xcursor-theme "Adwaita"
        xcursor-size 24
    }

    // a menu of the shell fades in, quickly. nothing else of the shell moves
    animations {
        layer-open {
            duration-ms 150
            curve "ease-out-quad"
        }
    }

    layer-rule {
        match namespace="^lens-menu$"
        match namespace="^lens-dialog$"
        animate-open true
    }

    // the console floats along the top of the working area, under lens's bar, full width. it has no
    // title bar: ghostty starts it with no decorations
    window-rule {
        match app-id=r#"^${lib.escapeRegex console.appId}$"#
        open-floating true
        open-focused true
        default-column-width { proportion 1.0; }
        default-window-height { fixed ${toString console.height}; }
        default-floating-position x=0 y=0 relative-to="top-left"
    }
    // settings opens wide enough for its sidebar and a page beside it
    window-rule {
        match app-id=r#"^dev\.rift\.Settings$"#
        default-column-width { proportion 0.75; }
    }
    // files opens wide enough for its places and a list with its columns beside them
    window-rule {
        match app-id=r#"^dev\.rift\.Files$"#
        default-column-width { proportion 0.75; }
    }
    // welcome opens over the desktop in the middle of the screen, at the size it asks for, and
    // nothing else moves for it
    window-rule {
        match app-id=r#"^dev\.rift\.Welcome$"#
        open-floating true
    }

    ${lib.concatMapStringsSep "\n" (
      command: "spawn-at-startup " + lib.concatMapStringsSep " " (word: ''"${word}"'') command
    ) cfg.startup}

    // the list of shortcuts shows the keys that are bound, each with a title of its own below
    hotkey-overlay {
        skip-at-startup
        hide-not-bound
    }

    // screen recordings are written beside these, in ~/Videos, by lens
    screenshot-path "~/Pictures/Screenshot %Y-%m-%d %H-%M-%S.png"

    binds {
        Mod+Shift+Slash hotkey-overlay-title="Show these shortcuts" { show-hotkey-overlay; }
        Mod+T hotkey-overlay-title="Open a terminal" { spawn "ghostty"; }
        Mod+Space hotkey-overlay-title="Show the Applications menu" { spawn "lens" "--menu"; }
        Mod+Shift+Space hotkey-overlay-title="Switch to the next keyboard layout" { switch-layout "next"; }
        Mod+Grave hotkey-overlay-title="Show or hide the console" { toggle-console app-id="${console.appId}" "${config.systemd.package}/bin/systemd-cat" "-t" "console" "ghostty" "--class=${console.appId}" "--window-decoration=none"; }
        // while the session is locked the key starts a lock screen again, in case the one that
        // locked it has gone. horizon turns a second one away while the first is still there
        Mod+L allow-when-locked=true hotkey-overlay-title="Lock the screen" { spawn ${
          lib.concatMapStringsSep " " (word: ''"${word}"'') lock
        }; }
        Mod+Q hotkey-overlay-title="Close the window" { close-window; }
        Mod+O repeat=false hotkey-overlay-title="Show all workspaces" { toggle-overview; }

        Mod+Left hotkey-overlay-title="Focus the column on the left" { focus-column-left; }
        Mod+Right hotkey-overlay-title="Focus the column on the right" { focus-column-right; }
        Mod+Up { focus-window-up; }
        Mod+Down { focus-window-down; }
        Mod+Ctrl+Left hotkey-overlay-title="Move the column left" { move-column-left; }
        Mod+Ctrl+Right hotkey-overlay-title="Move the column right" { move-column-right; }
        Mod+Ctrl+Up { move-window-up; }
        Mod+Ctrl+Down { move-window-down; }
        Mod+Home { focus-column-first; }
        Mod+End { focus-column-last; }

        Mod+Page_Down hotkey-overlay-title="Switch to the workspace below" { focus-workspace-down; }
        Mod+Page_Up hotkey-overlay-title="Switch to the workspace above" { focus-workspace-up; }
        Mod+Ctrl+Page_Down hotkey-overlay-title="Move the column to the workspace below" { move-column-to-workspace-down; }
        Mod+Ctrl+Page_Up hotkey-overlay-title="Move the column to the workspace above" { move-column-to-workspace-up; }
        Mod+1 { focus-workspace 1; }
        Mod+2 { focus-workspace 2; }
        Mod+3 { focus-workspace 3; }
        Mod+4 { focus-workspace 4; }
        Mod+5 { focus-workspace 5; }

        Mod+Comma { consume-window-into-column; }
        Mod+Period { expel-window-from-column; }
        Mod+R hotkey-overlay-title="Cycle the column width" { switch-preset-column-width; }
        Mod+F hotkey-overlay-title="Maximize the column" { maximize-column; }
        Mod+Shift+F hotkey-overlay-title="Full screen" { fullscreen-window; }
        Mod+C { center-column; }
        Mod+Minus { set-column-width "-10%"; }
        Mod+Equal { set-column-width "+10%"; }
        Mod+V hotkey-overlay-title="Float the window" { toggle-window-floating; }
        Mod+W hotkey-overlay-title="Tabs in the column" { toggle-column-tabbed-display; }

        Print hotkey-overlay-title="Take a screenshot" { screenshot; }
        Ctrl+Print { screenshot-screen; }
        Alt+Print { screenshot-window; }
        // lens starts and stops the recorder, marks the bar while it runs and names the file it
        // wrote when it stops. the same keys gnome uses
        Ctrl+Alt+Shift+R hotkey-overlay-title="Start or stop a screen recording" { spawn "lens" "--record"; }

        // push to talk. the shell records while it is listening, hands what was said to the model
        // that writes it down and puts the words in its field. it is a toggle, so repeat=false:
        // a key held down would otherwise start and stop it over and over
        Mod+H repeat=false hotkey-overlay-title="Start or stop listening" { spawn "lens" "--listen"; }

        // the screen reader on gnome's own keys, and the on-screen keyboard beside it. lens starts
        // and stops both, and the Applications menu has a row for each
        Mod+Alt+S hotkey-overlay-title="Turn the screen reader on or off" { spawn "lens" "--screen-reader"; }
        Mod+Alt+K hotkey-overlay-title="Show or hide the on-screen keyboard" { spawn "lens" "--keyboard"; }

        // lens makes the change with wpctl or brightnessctl and shows the level in its key popup
        XF86AudioRaiseVolume allow-when-locked=true { spawn "lens" "--volume" "up"; }
        XF86AudioLowerVolume allow-when-locked=true { spawn "lens" "--volume" "down"; }
        XF86AudioMute allow-when-locked=true { spawn "lens" "--volume" "mute"; }
        XF86MonBrightnessUp allow-when-locked=true { spawn "lens" "--brightness" "up"; }
        XF86MonBrightnessDown allow-when-locked=true { spawn "lens" "--brightness" "down"; }

        Mod+Escape allow-inhibiting=false { toggle-keyboard-shortcuts-inhibit; }
        Mod+Shift+E hotkey-overlay-title="End the session" { quit; }
        Ctrl+Alt+Delete { quit; }
    }

    // last, so the owner's theme and wallpaper, and the mouse and the touchpad, take the place of the
    // ones above
    include "${themePart}" optional=true
    include "${screensPart}" optional=true
    include "${pointerPart}" optional=true
  '';
in
{
  options.rift.horizon = {
    enable = lib.mkEnableOption "Horizon, the Rift compositor";
    user = lib.mkOption {
      type = lib.types.str;
      default = "rift";
      description = "the account the session runs as";
    };
    background = lib.mkOption {
      type = lib.types.str;
      default = "#242424";
      description = "the desktop background, a flat neutral gray under the wallpaper. the boot test looks for it";
    };
    wallpaper = lib.mkOption {
      type = lib.types.str;
      default = "/run/current-system/sw/share/backgrounds/rift/${wallpapers.default}.jpg";
      description = "the picture the desktop has until the owner picks another, by a path that stays the same across updates";
    };
    startup = lib.mkOption {
      type = lib.types.listOf (lib.types.listOf lib.types.str);
      default = [ ];
      example = [ [ "lens" ] ];
      description = "programs the compositor starts with the session, each as its argument list";
    };
  };

  config = lib.mkIf cfg.enable {
    services.greetd = {
      enable = true;
      # the owner's session starts once a boot with no password, the drive's passphrase was
      # asked for already. greetd opens it as a user session, which logind can lock. a default
      # session is a greeter to logind, and a greeter cannot lock
      settings.initial_session = {
        command = session;
        user = cfg.user;
      };
      # when the session ends, the owner's password on tty1 starts it again
      settings.default_session.command = "${config.services.greetd.package}/bin/agreety --cmd '${session}'";
      # greetd keeps a file in /run that says the first session ran, so a restart asks for the
      # password instead of starting it again
      restart = true;
    };
    # the lock screen. Mod+L runs it, and the listener runs it when logind signals the session,
    # which is what loginctl lock-session does. pam checks the owner's password
    rift.horizon.startup = [
      (lock ++ [ "--listen" ])
      # graphical-session.target, which user services of a graphical session need, xdg-desktop-portal
      # among them. horizon runs in greetd's session and not as a unit of its own that would bind it
      [
        "${config.systemd.package}/bin/systemctl"
        "--user"
        "start"
        "nixos-fake-graphical-session.target"
      ]
    ];
    security.pam.services.horizon-lock = { };
    # session files and XDG_DATA_DIRS for a greeter, nothing here reads them
    services.displayManager.enable = false;

    environment.etc."niri/config.kdl".source = configFile;
    # the layouts there are, with their names, which the Keyboard page lists. libxkbcommon reads the
    # same data from the store, and this is where other programs look for it
    environment.etc."X11/xkb".source = "${pkgs.xkeyboard-config}/etc/X11/xkb";
    # librift reads the system's wallpaper here when the owner has not picked one
    environment.etc."rift/wallpaper".text = "${cfg.wallpaper}\n";
    systemd.user.tmpfiles.rules = [
      "d %h/.config/ghostty 0755 - - -"
      "f %h/.config/ghostty/config.ghostty 0644 - - - ${ghosttySettings}"
    ];
    environment.systemPackages = [
      horizon
      pkgs.ghostty
      pkgs.wl-clipboard
      pkgs.brightnessctl
      pkgs.adwaita-icon-theme
      pkgs.hicolor-icon-theme
      adwaitaDark
      # the title bar qt 5 apps draw on wayland, in the adwaita style, dark or light with the desktop
      pkgs.qadwaitadecorations
      wallpapers
    ];
    # QT_PLUGIN_PATH to the plugins of the system's packages, where qt finds the decorations
    qt.enable = true;

    fonts.packages = [
      pkgs.noto-fonts
      pkgs.dejavu_fonts
    ];

    # one look for apps. gtk 3 and 4 and libadwaita read these through gsettings, and apps in a
    # flatpak through the gtk portal, which passes the colour scheme on. this is the system database
    # under the owner's own: the shell writes color-scheme and gtk-theme there from the theme
    # setting, dark by default. accent blue is libadwaita's own default, #78aeed on dark and #3584e4
    # on light. gtk 3 has no colour scheme and goes dark by the theme's name
    programs.dconf = {
      enable = true;
      profiles.user.databases = [
        {
          settings = {
            "org/gnome/desktop/interface" = {
              color-scheme = "prefer-dark";
              accent-color = "blue";
              gtk-theme = "Adwaita-dark";
              icon-theme = "Adwaita";
              cursor-theme = "Adwaita";
              cursor-size = lib.gvariant.mkInt32 24;
              font-name = "Noto Sans 11";
              document-font-name = "Noto Sans 11";
              monospace-font-name = "DejaVu Sans Mono 11";
            };
            # the close button alone, as gnome has it. horizon has no minimize
            "org/gnome/desktop/wm/preferences".button-layout = "appmenu:close";
          };
        }
      ];
    };
    environment.sessionVariables = {
      # the cursor for apps that do not read gsettings, and for the user manager, which lens starts
      # apps from. horizon's own cursor section says the same
      XCURSOR_THEME = "Adwaita";
      XCURSOR_SIZE = "24";
      # qt 5 and 6 take their colours, font, icons and file dialog from gtk. nixpkgs builds qtbase
      # with the gtk 3 platform theme, so this needs nothing more in the image
      QT_QPA_PLATFORMTHEME = "gtk3";
      # and their title bar from the adwaita decorations, not qt's own with its blue gradient
      QT_WAYLAND_DECORATION = "adwaita";
    };
    # the icon theme lens looks names up in, and the one gtk apps fall back to. the wallpapers go
    # under /run/current-system/sw too, where a path to one outlives the store path it came in
    environment.pathsToLink = [
      "/share/icons"
      "/share/backgrounds"
    ];
    fonts.fontconfig.defaultFonts = {
      sansSerif = [ "Noto Sans" ];
      monospace = [ "DejaVu Sans Mono" ];
    };
  };
}
