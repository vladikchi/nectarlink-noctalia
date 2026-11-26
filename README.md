# NectarLink for Noctalia

NectarLink pairs your Android phone with your Linux desktop. This fork extends the local API used by the [NectarLink plugin for Noctalia](https://github.com/vladikchi/nectarlink-plugins). The plugin can show battery status, notifications and recent activity; send SMS; manage calls; browse photos; and start supported screen, webcam and audio sessions.

## Installation on Linux

The [latest published release](https://github.com/vladikchi/nectarlink-noctalia/releases/latest) includes `.deb`, `.rpm`, tarball, and AppImage packages for Linux x86-64. The source on this branch may be newer than the published packages.

### Installer (`.deb`, `.rpm`, or tarball)

```sh
curl -fsSL https://github.com/vladikchi/nectarlink-noctalia/releases/latest/download/install.sh | sh
```

The installer selects a `.deb` for APT, an `.rpm` for DNF or Zypper, or a tarball otherwise. It downloads packages from this fork and verifies the SHA-256 checksum before installing.

### AppImage

Download the Linux x86-64 AppImage from the [latest published release](https://github.com/vladikchi/nectarlink-noctalia/releases/latest), make it executable, and run it. No system installation is required. For example, if you save it as `NectarLink.AppImage`:

```sh
chmod +x ./NectarLink.AppImage
./NectarLink.AppImage
```

### Build from source

Install Rust, the Qt 6 development tools, a C++ compiler, CMake, Ninja, and the system libraries. From the repository root:

```sh
cargo build --release --locked --package nectarlink-desktop
./target/release/nectarlink-desktop
```

## Phone and plugin setup

Use the official upstream APK matching your desktop build: [Nightly 13](https://github.com/thtbee/nectarlink/releases/tag/v0.1.6-nightly.13) for this source branch, or [Nightly 12](https://github.com/thtbee/nectarlink/releases/tag/v0.1.6-nightly.12) for the published Nightly 12 build. This fork does not ship a modified Android app.

1. Start NectarLink Desktop and pair your phone.
2. Under **Settings → Connection & About**, enable **Local automation API**. If you want the plugin to read notifications and messages, also enable **Notifications and messages**.
3. Follow the [plugin setup guide](https://github.com/vladikchi/nectarlink-plugins#installation) to add the widget in Noctalia. Run both applications as the same user and use NectarLink's default profile.

## What this fork adds

- **Local API for the Noctalia panel.** The API listens on `127.0.0.1` and requires a local token. Notification and message access is off by default and respects notification visibility rules.
- **Faster browsing.** Messages, photos and app lists can reuse cached results. When supported by the phone, the panel can show an estimate of the time remaining until full charge.
- **Noctalia colors in Hive.** The Hive theme can use an existing `~/.config/nectarlink/noctalia-colors.json`. If no valid palette is available, it uses its built-in colors.

## Source and licenses

Based on upstream [NectarLink `v0.1.6-nightly.13`](https://github.com/thtbee/nectarlink/tree/v0.1.6-nightly.13). The `android/` source tree is unchanged from that tag. See [the upstream project](https://github.com/thtbee/nectarlink) and [component licenses](LICENSE).
