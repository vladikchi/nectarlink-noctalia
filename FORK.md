# NectarLink × Noctalia — Linux integration fork

Unofficial Linux integration based on NectarLink v0.1.6-nightly.10; desktop
version 0.1.5+noctalia.1. Not endorsed by upstream or Noctalia. The version follows
the original Cargo.toml (0.1.5); the source archive/tag remains v0.1.6-nightly.10.

## Scope

- External Noctalia palette loader and palette watcher; native Hive animation
  retained. Color integration is optional.
- Extended authenticated loopback API for the separately distributed Noctalia
  plugin: messages, calls, photos, apps, Deck and session/phone controls.
- Bounded read caches, private previews and control validation.
- Selected Linux audio/mirror fixes. The standard variant preserves upstream Island.

The desktop window is named "NectarLink — Noctalia Integration". The palette
loader is read-only; the color toggle is saved by ordinary Preferences as
`noctaliaColors`. Legacy palette/.disabled state is used only when this setting
is absent. The loader no longer writes markers or reads NECTARLINK_PALETTE.
Play-on-phone offers retain upstream's temporary validity after stopping;
the fork-specific offer-revocation method has been removed. API device targeting
and local preparation-generation guards remain.

The local API is organized under `desktop/app/src/local_api/`: transport/token
handling, shared request types and feature-specific handlers are separated.
The facade keeps the existing lifecycle and routes. This structural refactor
does not change v1 URLs, JSON, command confirmation or retry policy.

Only Linux is the target of these changes. Other upstream platform sources
remain for provenance; they have not been validated for this fork.
Upstream update checks and installation are disabled for Linux. Update by
replacing the AppImage with a newer build of this fork while NectarLink is closed.

## Build

This variant provides the Noctalia palette and extended API, but keeps the
upstream Island, keyboard layout detection, and Wayland capture implementation.
Niri-targeted capture, keyboard-layout, and Island fixes belong to
`nectarlink_niri` and `nectarlink_common`, not this variant. On stock Niri 26.04, the
retained idle window can block clicks underneath. Use the separate local
`nectarlink_common` project if you need both the plugin and the Niri fixes.

Follow the upstream Linux dependency instructions in this source tree (Rust,
Qt 6 development tools, CMake, Ninja, C++ compiler and platform libraries).
Then build from the repository root:

```sh
cargo build --release --locked --package nectarlink-desktop
```

Qt tool names/locations vary by distribution; set QMAKE/LRELEASE and PATH when
required. For a distributable x86-64 AppImage, run the **Build** workflow manually
on the fork branch and download the `linux` artifact. This builds against Ubuntu
24.04, includes Qt and Russian translations, and does not publish a release.
A build produced on a newer Linux host is suitable for local testing, not a
substitute for the compatibility build and clean-system tests.

## Connect the plugin

Pair the phone in the native app, enable its local API in settings, and install
the companion plugin following its README. The plugin currently expects the
default `~/.local/share/Nectarlink/` profile. Keep the API bound to loopback;
never distribute the generated token, identity, database or personal previews.
No pairing identity or profile is included in these sources.

Existing Android permissions are still required. Basic message/photo/app
integration does not imply full parity with the native app or full messenger
history. Android permissions are not bypassed.

## Licenses and release status

Preserve upstream LICENSE, LICENSES/ and third-party notices. Desktop changes
follow the desktop's GPL-3.0-or-later terms; modified core files follow the
core's MPL-2.0 terms. See upstream's license mapping and exceptions, not a
single blanket license for the entire repository.

This is a source candidate, not a completed independent security audit. The
upstream archive hash is recorded in the accompanying manifest, but upstream
authenticity and a clean-machine build still require verification before a
public release. Public author/repository/update-channel names are not chosen.

## This source snapshot

Application variant: **standard**. The companion Noctalia plugin is shared.
