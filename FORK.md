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
upstream Island and keyboard layout detection. On stock Niri 26.04, the
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

### Wayland PC-to-phone capture backport (2026-11-17)

Selected desktop capture changes from upstream v0.1.6-nightly.11 / PR #403
(https://github.com/thtbee/nectarlink/pull/403) are backported, not the entire
nightly release. The extended API, palette integration and standard Island
behavior are retained. The application version remains 0.1.5+noctalia.1.

Each capture attempt owns its portal session and PipeWire process. Portal setup
has a 20-second deadline and cancellation closes pending requests. Missing frames,
closed sessions and encoder stalls end the attempt with a logged error. Input,
when the portal supports it and the phone's remote-input setting allows it, uses
the same permission session. The `pc_capture_failed` notice is understood by
upstream Android nightly.11; older Android clients may not display its retry UI.

Two local safeguards supplement the backport: failed portal availability probes
are not cached for the process lifetime, and desktop capture under Wayland does
not silently substitute the Xwayland root window if the portal is unavailable.
No compositor configuration, pairing identity or API authentication is changed.

The capture thread also keeps Tokio entered for the complete portal lifetime,
including teardown: zbus schedules subscription cleanup during `Drop`. Capture
errors are logged before teardown and GStreamer stderr goes to the launcher's
journal instead of being discarded. An isolated private-D-Bus regression test
exercises subscription cleanup from an ordinary capture-style thread.

On systems with GStreamer's `glupload`, `glcolorconvert` and `gldownload`, the
PipeWire capture pipeline imports GPU/DMABuf frames through OpenGL and downloads
plain RGBA before CPU conversion/scaling. Without these optional elements the
original CPU pipeline remains available. This addresses DRM-modifier negotiation
failures on the tested Niri/NVIDIA desktop: an isolated live portal session
reproduced zero frames with the CPU pipeline and received three complete 1280x720
frames with the GL pipeline, without recording or transmitting desktop content.
