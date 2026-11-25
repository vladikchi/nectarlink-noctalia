# NectarLink for Noctalia

NectarLink pairs your Android phone with your Linux desktop. This fork extends the local API used by the [NectarLink plugin for Noctalia](https://github.com/vladikchi/nectarlink-plugins). The plugin can show battery status, notifications and recent activity; send SMS; manage calls; browse photos; and start supported screen, webcam and audio sessions.

## Installation on Linux

Get a Linux build from [GitHub Releases](https://github.com/vladikchi/nectarlink-noctalia/releases). For the source version in this branch, look for `0.1.6-nightly.12+noctalia`. The older `0.1.5+noctalia.2` release does not include the newer API features described below.

On your phone, install the [official NectarLink Nightly 12 APK](https://github.com/thtbee/nectarlink/releases/tag/v0.1.6-nightly.12). This fork does not ship a modified Android app.

1. Start NectarLink Desktop and pair your phone.
2. Under **Settings → Connection & About**, enable **Local automation API**. If you want the plugin to read notifications and messages, also enable **Notifications and messages**.
3. Follow the [plugin setup guide](https://github.com/vladikchi/nectarlink-plugins#installation) to add the widget in Noctalia. Run both applications as the same user and use NectarLink's default profile.

## What this fork adds

- **Local API for the Noctalia panel.** The API listens on `127.0.0.1` and requires a local token. Notification and message access is off by default and respects notification visibility rules.
- **Faster browsing.** Messages, photos and app lists can reuse cached results. When supported by the phone, the panel can show an estimate of the time remaining until full charge.
- **Noctalia colors in Hive.** The Hive theme can use an existing `~/.config/nectarlink/noctalia-colors.json`. If no valid palette is available, it uses its built-in colors.

## Source and licenses

Based on upstream [NectarLink `v0.1.6-nightly.12`](https://github.com/thtbee/nectarlink/tree/v0.1.6-nightly.12). The `android/` source tree is unchanged from that tag. See [the upstream project](https://github.com/thtbee/nectarlink) and [component licenses](LICENSE).
