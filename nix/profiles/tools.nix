# the command line tools and the languages every image has, so a new drive can build and debug
# software with no network. git, ripgrep, fd and btop are in base.nix, the ssh client comes with
# nixos, and the apps with windows are in apps.nix
{ pkgs, ... }:
{
  environment.systemPackages = with pkgs; [
    neovim
    gh
    # debuggers and tracers. perf is built from the same kernel sources as the image's kernel
    gdb
    lldb
    valgrind
    ltrace
    perf
    # the terminal
    fzf
    bat
    # network and security
    nmap
    wireguard-tools
    age
    # the hardware. nvtop for every gpu family but nvidia, whose counters come from the proprietary
    # driver the image does not have
    lm_sensors
    smartmontools
    powertop
    (nvtopPackages.full.override { nvidia = false; })

    # rust
    rustc
    cargo
    rustfmt
    clippy
    rust-analyzer
    # c and c++. gcc is cc, clang and lld are beside it with the llvm tools and clangd
    (lib.hiPrio gcc)
    clang
    lld
    llvm
    clang-tools
    cmake
    ninja
    gnumake
    # the other languages
    python3
    nodejs
    bun
    go
    zig
    jdk25
  ];

  # java is the current long term release. JAVA_HOME points at it for gradle, maven and the editors,
  # in the session as well as in shells
  environment.sessionVariables.JAVA_HOME = pkgs.jdk25.home;

  # gpg asks for a passphrase in a gtk dialog in the session and on the terminal without one
  programs.gnupg.agent = {
    enable = true;
    pinentryPackage = pkgs.pinentry-gnome3;
  };

  # iotop-c as iotop, with the capability it needs to read the counters without sudo, and
  # the kernel's delay accounting for its swap and io columns
  programs.iotop = {
    enable = true;
    package = pkgs.iotop-c;
    enableDelayacct = true;
  };
}
