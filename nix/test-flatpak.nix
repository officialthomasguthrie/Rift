# a flatpak runtime and app of our own for the boot test, in a repository the test serves and adds as
# a remote, so welcome installs the app the way it installs one from flathub and the test needs
# nothing from flathub. the runtime is a static busybox and a static gdbus, the app one shell script
{ pkgs }:
let
  inherit (pkgs) lib;
  runtime = "dev.rift.TestPlatform";
  app = "dev.rift.TestApp";
  # the name the applications menu lists the installed app under
  appName = "Rift test app";
  branch = "test";
  # what the app does: reads the file the document portal gave it, then the same file where it is in
  # home, asks the desktop portal whether there is a network, and asks the user manager for
  # something, which the bus proxy flatpak puts in front of the session bus does not pass on
  probe = pkgs.writeText "probe" ''
    #!/bin/sh
    document=$1
    direct=$2
    echo "cgroup: $(cat /proc/self/cgroup)"
    cat "$document" && echo document-read
    cat "$direct" && echo direct-read
    gdbus call --session --timeout 10 --dest org.freedesktop.portal.Desktop \
      --object-path /org/freedesktop/portal/desktop \
      --method org.freedesktop.portal.NetworkMonitor.GetAvailable && echo portal-answered
    gdbus call --session --timeout 10 --dest org.freedesktop.systemd1 \
      --object-path /org/freedesktop/systemd1 \
      --method org.freedesktop.DBus.Peer.Ping && echo manager-answered
    echo finished
  '';
in
pkgs.runCommand "rift-test-flatpak"
  {
    nativeBuildInputs = [
      pkgs.flatpak
      pkgs.gnupg
    ];
  }
  ''
    export HOME=$TMPDIR

    # the system installation takes nothing over http from a remote that is not signed, so the
    # repository is, with a key made here and thrown away with the build. the test imports the public
    # half when it adds the remote
    export GNUPGHOME=$TMPDIR/gnupg
    mkdir -m 700 $GNUPGHOME
    gpg --batch --pinentry-mode loopback --passphrase "" \
      --quick-generate-key "Rift test <test@rift.invalid>" rsa2048 sign never
    key=$(gpg --list-keys --with-colons | awk -F: '/^fpr/ { print $10; exit }')
    sign="--gpg-sign=$key --gpg-homedir=$GNUPGHOME"

    # a runtime's files are its usr, and build-export wants the files folder a build-init makes as well
    mkdir -p platform/usr/bin platform/files
    cp ${pkgs.pkgsStatic.busybox}/bin/busybox platform/usr/bin/busybox
    for tool in $(platform/usr/bin/busybox --list); do
      [ -e platform/usr/bin/$tool ] || ln -s busybox platform/usr/bin/$tool
    done
    cp ${lib.getBin pkgs.pkgsStatic.glib}/bin/gdbus platform/usr/bin/gdbus
    cat > platform/metadata <<EOF
    [Runtime]
    name=${runtime}
    runtime=${runtime}/x86_64/${branch}
    sdk=${runtime}/x86_64/${branch}
    EOF
    flatpak build-export $sign --runtime --disable-fsync repo platform ${branch}

    mkdir -p testapp/files/bin testapp/export/share/applications
    install -m 755 ${probe} testapp/files/bin/probe
    # an exported desktop entry, so the test can see an installed flatpak in the applications menu
    cat > testapp/export/share/applications/${app}.desktop <<EOF
    [Desktop Entry]
    Type=Application
    Name=${appName}
    Exec=probe
    Icon=${app}
    Categories=Utility;
    EOF
    cat > testapp/metadata <<EOF
    [Application]
    name=${app}
    runtime=${runtime}/x86_64/${branch}
    sdk=${runtime}/x86_64/${branch}
    command=probe

    [Context]
    shared=network;
    EOF
    flatpak build-export $sign --disable-fsync repo testapp ${branch}
    # the summary a client reads first, signed as well
    flatpak build-update-repo $sign repo

    mkdir -p $out
    cp -r repo $out/repo
    gpg --export $key > $out/key.gpg
    gpgconf --kill gpg-agent
  ''
