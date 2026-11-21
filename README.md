# NectarLink for Noctalia

[NectarLink](https://github.com/thtbee/nectarlink) connects your Android phone to your computer to transfer files, sync the clipboard, and mirror the phone's screen. This build extends the local API so the [NectarLink plugin](https://github.com/vladikchi/nectarlink-plugins) can read notifications, send SMS, manage calls, and start screen mirroring or webcam sessions from Noctalia.

**Use this build with the plugin.** Upstream NectarLink does not provide the API routes the plugin needs. This fork also lets the Hive theme use Noctalia's color palette.

## Installation on Linux

The [latest release](https://github.com/vladikchi/nectarlink-noctalia/releases/latest) includes `.deb`, `.rpm`, tarball, and AppImage packages for Linux x86-64.

### Installer (`.deb`, `.rpm`, or tarball)

Install with one command:

```sh
curl -fsSL https://github.com/vladikchi/nectarlink-noctalia/releases/latest/download/install.sh | sh
```

The installer selects a `.deb` for APT, an `.rpm` for DNF or Zypper, or a tarball otherwise. It verifies the package's SHA-256 checksum before installation. Packages are downloaded from this fork, not from upstream NectarLink.

### AppImage

[Download the Linux x86-64 AppImage](https://github.com/vladikchi/nectarlink-noctalia/releases/latest), make it executable, and run it. No system installation is required. For example, for `0.1.5+noctalia.2`:

```sh
chmod +x ./Nectarlink-0.1.5-noctalia.2-x86_64.AppImage
./Nectarlink-0.1.5-noctalia.2-x86_64.AppImage
```

### Build from source

You need Rust, Qt 6 development tools, a C++ compiler, CMake, Ninja, and the system libraries. From the repository root:

```sh
cargo build --release --locked --package nectarlink-desktop
./target/release/nectarlink-desktop
```

**Android:** [download the official NectarLink APK](https://github.com/thtbee/nectarlink/releases/download/v0.1.5/Nectarlink-0.1.5-android.apk). Use the upstream Android app on the phone; the fork is only needed on the PC.

The [plugin installation guide](https://github.com/vladikchi/nectarlink-plugins#installation) covers pairing, enabling the API, and adding the widget.

## What's changed from upstream

- **Local API:** adds notification and panel-section reads, phone commands, and session controls. For camera and data-transfer requests, an invalid explicit target is rejected rather than replaced with another phone.
- **Noctalia palette:** the Hive theme can read an existing `~/.config/nectarlink/noctalia-colors.json`. NectarLink does not create this file. Hive falls back to its built-in colors if no valid palette is available.
- **Updates and packaging:** upstream updates are disabled on Linux. Before updating manually, quit NectarLink, then replace the AppImage or install a newer package from this fork. Your profile is stored separately. Linux packaging checks that the Russian translation is included.

The Island, phone-to-PC typing, and screen capture fixes for Niri are in [NectarLink for Niri](https://github.com/vladikchi/nectarlink-niri). They are not part of this build.

[Full changes, API routes, and source files](CHANGES.md)

## Verification and licensing

The local AppImage passed `--version` and `--ui-check` on Arch Linux, and the Russian translation loaded. That AppImage check did not cover a real phone, the Wayland portal, or other distributions.

Based on NectarLink `v0.1.6-nightly.10`. [Upstream project](https://github.com/thtbee/nectarlink) · [Component licenses](LICENSE)
