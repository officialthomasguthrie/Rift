<div align="center">

<img alt="Rift logo" src="nix/liftoff/logo/rift-mark.png" width="260">

# Rift

A portable operating system that runs from a USB drive, keeps its owner's data encrypted on that drive, and leaves the host computer's disks untouched.

<p>
<a href="https://github.com/officialthomasguthrie/Rift/actions/workflows/ci.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/officialthomasguthrie/Rift/ci.yml?branch=main&style=flat-square&label=build&logo=githubactions&logoColor=white&labelColor=2e2e2e"></a>
<img alt="Version" src="https://img.shields.io/badge/version-0.1.0%20pre--release-3584e4?style=flat-square&labelColor=2e2e2e">
<a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/license-GPL--3.0--or--later-3584e4?style=flat-square&labelColor=2e2e2e"></a>
<a href="https://github.com/officialthomasguthrie/Rift/commits/main"><img alt="Last commit" src="https://img.shields.io/github/last-commit/officialthomasguthrie/Rift/main?style=flat-square&label=last%20commit&labelColor=2e2e2e&color=555555"></a>
</p>

<p>
<img alt="Platform" src="https://img.shields.io/badge/platform-x86--64%20UEFI-555555?style=flat-square&logo=linux&logoColor=white&labelColor=2e2e2e">
<img alt="Built with Nix" src="https://img.shields.io/badge/built%20with-Nix-555555?style=flat-square&logo=nixos&logoColor=white&labelColor=2e2e2e">
<img alt="Written in Rust" src="https://img.shields.io/badge/written%20in-Rust-555555?style=flat-square&logo=rust&logoColor=white&labelColor=2e2e2e">
<img alt="Wayland" src="https://img.shields.io/badge/display-Wayland-555555?style=flat-square&logo=wayland&logoColor=white&labelColor=2e2e2e">
<img alt="Flatpak" src="https://img.shields.io/badge/apps-Flathub-555555?style=flat-square&logo=flathub&logoColor=white&labelColor=2e2e2e">
</p>

<a href="#overview">Overview</a> &nbsp;|&nbsp;
<a href="#features">Features</a> &nbsp;|&nbsp;
<a href="#system-requirements">Requirements</a> &nbsp;|&nbsp;
<a href="#installation">Installation</a> &nbsp;|&nbsp;
<a href="#building-from-source">Building</a> &nbsp;|&nbsp;
<a href="#project-status">Status</a> &nbsp;|&nbsp;
<a href="#license">License</a>

</div>

## Overview

Rift is a general purpose desktop operating system designed to live on a portable drive rather than inside a particular computer. The drive holds two things that are kept strictly apart: a read-only system image that is verified as it is read, and an encrypted volume that holds everything belonging to the owner, including files, applications, AI models and per-machine settings.

Booted on a compatible PC, Rift uses that machine's processor, memory, graphics, display and network, and presents the same desktop, files and configuration on every computer it is used with. The internal disks of the host are not mounted, and nothing is written to them.

The system is assembled from nixpkgs with Nix, so each image is fully described by this repository and its lock file. Every component written for Rift is implemented in Rust.

## Features

### Immutable system and atomic updates

- The operating system is a read-only Nix store in an EROFS file system, compressed with zstd and protected by dm-verity. A block that does not match its hash is refused rather than read.
- The drive carries two system slots. systemd-sysupdate writes a new version into the inactive slot, so the running version is never modified during an update.
- systemd-boot counts boot attempts for each new version. A version that fails to boot three times is set aside and the previous version starts again.
- Each version boots as a unified kernel image on UEFI firmware, with the current stable Linux kernel.

### Encrypted storage

- Personal data lives on a LUKS2 volume formatted with btrfs and zstd compression, divided into subvolumes by kind of data.
- The volume is created on the first boot of a newly written drive and unlocked with a passphrase at each boot after that.
- Memory pressure is absorbed by compressed RAM (zram). No swap space exists on the drive or on the host.
- An optional exFAT partition provides ordinary storage that Windows, macOS and other Linux systems can read without Rift.

### Snapshots, backup and cloning

- The home directory is snapshotted every hour with configurable retention, and individual files can be restored from any snapshot.
- Backups are encrypted and deduplicated with rustic, in the restic repository format, to an external disk.
- A complete, bootable copy of the drive can be written to a second drive, which receives an encryption key of its own.

### Desktop

- Horizon, a Wayland compositor derived from niri, arranges windows as columns on a horizontally scrolling strip for each display.
- Lens, the desktop shell, draws a top bar with a clock menu that holds a calendar, the notifications received so far and a Do not disturb switch, and a system menu for sound, brightness, wired and Wi-Fi networks, Bluetooth, the battery and the session; an applications menu that lists installed applications by category, with a search field that also runs system commands, evaluates Nushell pipelines and passes requests written in plain language to the local assistant; and a dock that holds pinned and running applications and the workspaces.
- Lens is the notification server of the session, following the freedesktop.org notification specification, and shows the level when a volume or brightness key is pressed.
- A drop-down terminal and a lock screen are built into the compositor.
- The Print key takes a screenshot, and Ctrl, Alt, Shift and R start and stop a screen recording. Recordings are encoded in software and written as MP4 files, so they do not depend on the graphics hardware of the machine.
- Orca, the GNOME screen reader, and an on-screen keyboard are included, each turned on from the keyboard or from the applications menu. Speech is synthesised on the machine with eSpeak NG, and the accessibility bus is enabled for every application.
- The desktop is dark by default and can be switched to light. GTK, libadwaita and Qt applications follow the same setting in the Adwaita style with the accent colour chosen in Settings, Noto Sans and the Adwaita icons and cursor, and draw their own title bars.
- Settings covers the system in one window with a sidebar of pages. Appearance sets the wallpaper, dark or light, one of nine accent colours, the interface text size, the gap between windows, the corner radius of a window, the terminal colour scheme, the terminal greeting and the style of the boot, and a change takes effect at once. Displays shows each screen and the size it is drawn at. Wi-Fi turns the radio on and off, lists the networks around and joins one. Network shows the wired connection and its addresses. Bluetooth turns the adapter on and connects a device that is paired. Sound sets the volume of the output and the input, mutes either one and picks the device each of them uses. Power shows the battery, what it is doing and how long it has. Search shows what the index for search by meaning holds and brings it up to date. Keyboard sets the layouts the desktop types with, which are kept on the drive, and shows the keyboard shortcuts. Mouse and touchpad sets the primary button, the pointer speed, acceleration and the scrolling direction, and on a touchpad tapping, edge scrolling and ignoring the touchpad while typing. Printers lists the printers found on the network and the jobs waiting for them, sets the one applications print to first, resumes a stopped printer and cancels a job. Accessibility turns the screen reader and the on-screen keyboard on and off and sets the size of the text. Date and time shows the clock and sets the time zone, which is kept on the drive. Region and language shows the language, the formats and the keyboard layouts. Updates shows the version in each of the drive's two slots and whether a newer one is waiting. Backups shows the snapshots of the home folder and the backups on a disk, and makes a new one of either. AI shows the model the local assistant runs and sets the size of model the machine asks for. About shows the version of the system and what Rift has found out about the machine. The pages that are not written yet are listed and say so.
- The wallpaper is one of nine NASA photographs included in the image, each with its source and license in a text file beside it, or any JPEG or PNG picture, or a flat colour. Horizon scales it to fill each display. The default is a photograph of the night side of Earth taken from the Orion spacecraft on Artemis II.
- Flatpak is integrated with xdg-desktop-portal for applications installed from Flathub.

### Applications and development tools

The image includes the following software, so a new drive is usable without a network connection.

- Firefox, Ghostty, Zed, Helix, Neovim, KeePassXC and virt-manager, with fish, bash, Nushell and zellij.
- GNOME's viewers and utilities for everyday files: Loupe for pictures, Papers for documents, Showtime for video, Decibels for sound, Calculator, File Roller for archives, Disks, Disk Usage Analyzer, Characters, Camera and Document Scanner.
- Rootless Podman, which also answers to `docker`, and QEMU with KVM through libvirt, with UEFI firmware and a software TPM for guests.
- Compilers and runtimes for Rust, C and C++ (GCC, Clang and LLVM, with CMake, Ninja and Make), Python, Node.js, Bun, Go, Zig and Java.
- gdb, LLDB, Valgrind, strace, ltrace and perf for debugging, and git, the GitHub CLI, ripgrep, fd, fzf and bat.
- nmap, OpenSSH, WireGuard tools, GnuPG and age for networks and encryption, and lm_sensors, smartmontools, PowerTOP, iotop, nvtop and btop for the hardware.
- A user guide is part of the image, as pages that open in the browser with no network connection, and `rift guide` opens it from a terminal.

### Printers, scanners and firmware

- Printing is driverless. CUPS prints to IPP Everywhere and AirPrint printers, which describe their own capabilities, and no printer manufacturer's software is installed. Printers on the local network are found with Avahi.
- Scanning uses SANE, including the driverless eSCL and WSD backends for scanners on the network.
- Rift asks the local network for printers and scanners and announces nothing about itself on it.
- fwupd reports the firmware of the machine: each device that has firmware, its version, and whether Secure Boot, the TPM and the IOMMU are in use. Firmware is never written, because the computer Rift is running on may not belong to its owner.

### Per-machine configuration

- Orbit identifies each host from its DMI and PCI identifiers and stores a profile for it on the encrypted volume. A machine that has been seen before is configured from its profile.
- On a machine it has not seen, it derives display scaling from the physical size reported by the monitor, selects a graphics path, and chooses an AI model tier suited to the available memory.
- Every host is classed as owned, trusted or borrowed. Internal disks are never mounted automatically.

### Local AI

- Quasar runs language models with llama.cpp on the host itself, using the GPU where one is usable and the CPU otherwise. No account, network connection or remote service is involved.
- Models from the Qwen3 family, licensed under Apache-2.0, are selected by tier according to host memory. Each model is declared with its checksum in `models/manifest.toml`, and weights are stored on the encrypted volume.
- An OpenAI-compatible API on `127.0.0.1:11434` makes the models available to local programs. Requests that originate from web pages are refused.
- A request made through the shell returns either an answer or a proposed action, and no action runs until it is confirmed.
- Files in the home directory can be searched by meaning rather than by exact wording, through an embedding index that is built and kept on the drive.

### Application sandboxing

- Command-line programs can be run in a bubblewrap sandbox confined by Landlock filesystem rules and a seccomp filter.
- A per-application network switch, implemented with nftables, removes network access from a sandboxed program.
- Flatpak applications run in their own sandboxes and reach the desktop through portals.

### Privacy

Rift contains no telemetry, requires no account, and depends on no online service to boot or to operate.

## Components

| Component | Function |
|---|---|
| Liftoff | System image, boot process and updates |
| Horizon | Wayland compositor |
| Lens | Desktop shell: top bar, menus, dock and notifications |
| Quasar | Local AI service and API |
| Orbit | Host detection and per-machine profiles |
| Airlock | Application sandboxing and network control |
| Vault | Snapshots, backup and cloning |
| rift-flash | Drive writer for Windows, macOS and Linux |
| librift | Shared library used by the tools above |

System services are exposed on D-Bus under `dev.rift.*`.

## Command line

The `rift` command reaches the same services as the desktop.

| Command | Purpose |
|---|---|
| `rift host` | Show the stored profile of the current machine |
| `rift ai` | Ask a question, or index and search the home directory by meaning |
| `rift doctor` | Check the drive, the host and the system services |
| `rift snapshot` | List, take and restore from snapshots |
| `rift backup` | List, make and restore from encrypted backups |
| `rift clone` | Write a complete second drive with a new key |
| `rift run --sandbox` | Run a command inside a sandbox |
| `rift net` | Turn network access off or on for a sandboxed application |
| `rift wallpaper` | List the wallpapers, or set a photograph, a picture or a colour |
| `rift guide` | Open the user guide that comes with the system |

## System requirements

| | Minimum | Recommended |
|---|---|---|
| Processor | 64-bit x86 (x86-64) | A recent multi-core processor |
| Firmware | UEFI | UEFI |
| Memory | 4 GB | 8 GB or more |
| Drive capacity | 64 GB | 256 GB |
| Drive type | USB 3.0 flash drive | SSD-class USB drive, or an NVMe SSD in a USB 3.2 Gen 2 enclosure |

Secure Boot must currently be disabled in the firmware settings. Legacy BIOS, 32-bit processors and Apple Silicon Macs are not supported.

The drive determines most of the system's responsiveness. Low-cost flash drives will boot, but they are slow under the sustained random writes of a full operating system and wear out quickly.

## Installation

No release has been published yet. Installation images, together with their checksums, will be listed on the [releases page](https://github.com/officialthomasguthrie/Rift/releases) when the first version is available.

Drives are written with rift-flash, a graphical application and command-line tool for Windows, macOS and Linux. It only offers removable drives, and it refuses any disk that is internal, in use, or running the current system.

## Building from source

Building requires [Nix](https://nixos.org/download/) with flakes enabled. The image itself must be built on an x86_64-linux machine or through a remote builder of that type. The Rust workspace builds on Linux, macOS and Windows, apart from the compositor, which requires Linux.

```sh
git clone https://github.com/officialthomasguthrie/Rift.git
cd Rift

nix build .#image   # build the bootable disk image
just vm             # boot the image in QEMU with KVM

just build          # build the Rust workspace
just test           # run the test suite
just lint           # rustfmt and clippy
```

Every change to `main` is built and tested by continuous integration. The pipeline builds the full image and boots it in QEMU, then exercises unlocking, an update, a rollback, snapshots, a backup, a clone, the sandbox, the local AI and the included compilers inside the running system.

## Project status

Rift is in active development ahead of its first public release.

| Area | State |
|---|---|
| Verified system image, A/B updates and automatic rollback | Working |
| Encrypted storage created on first boot | Working |
| Compositor, shell, drop-down terminal and lock screen | Working |
| Per-machine profiles | Working |
| Local AI, API and search by meaning | Working |
| Snapshots, backup and cloning | Working |
| rift-flash for Windows, macOS and Linux | Working |
| Sandboxing, network switch and Flatpak with portals | Working |
| Distribution branding and text boot | In progress |
| Top bar, application menu, dock, system menu and notifications | Working |
| Settings, first-run setup, file manager and software center | Planned |
| Voice input and speech output | Planned |
| FIDO2 unlock, TPM2 unlock on owned machines, and a mode that boots without personal data | Planned |
| Signed releases, Secure Boot support and delta updates | Planned |

## Reporting problems

Bugs and feature requests are tracked in [GitHub issues](https://github.com/officialthomasguthrie/Rift/issues). Reports of how Rift runs on specific hardware are especially useful; the form for them is [`hw/TEMPLATE.md`](hw/TEMPLATE.md).

## License

Rift is licensed under the [GNU General Public License, version 3 or later](LICENSE). librift is licensed under the [Apache License 2.0](LICENSE-APACHE) so that other software can use it. Horizon is derived from [niri](https://github.com/niri-wm/niri) by Ivan Molodetskikh and the niri contributors. Third-party software included in the image remains under its own license.

Copyright 2026 Thomas Guthrie.
