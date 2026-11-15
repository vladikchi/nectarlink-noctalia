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
The upstream update mechanism has not yet been replaced with a fork channel.
Disable automatic upstream updates in the app's settings for this candidate:
an official update can replace the extended API with an incompatible build.
A safe fork updater policy is still a public-release blocker; updates should
be installed manually until that policy/channel is implemented.

## Build

Two Linux source variants are provided with the same API, profile identity and
Noctalia plugin. Install only one at a time:

- **standard**: IslandWindow.qml is byte-for-byte the supplied upstream version.
  No Niri workaround is applied. Hyprland behavior has not been validated here.
  On stock Niri 26.04, the retained idle window can block clicks underneath.
- **niri**: the Island window is unmapped immediately when idle. This avoids
  blocked clicks without a custom compositor, but loses idle window retention
  and may lose a manually chosen position on the next appearance. Optional Niri
  placement rules can restore a fixed position; the application cannot guarantee it.

The experimental compositor patch is separate, optional and not installed.
Neither application variant requires a modified Niri. No automatic compositor
detection or runtime switching is introduced.

Follow the upstream Linux dependency instructions in this source tree (Rust,
Qt 6 development tools, CMake, Ninja, C++ compiler and platform libraries).
Then build from the repository root:

```sh
cargo build --release --locked --package nectarlink-desktop
```

Qt tool names/locations vary by distribution; set QMAKE/LRELEASE and PATH when
required. No distribution-independent binary package is provided by this source
candidate. Install the resulting `nectarlink-desktop` in PATH, or select its
absolute path in the plugin settings.

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
