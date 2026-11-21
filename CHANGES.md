# NectarLink changes for Noctalia

This document describes the Noctalia fork's changes relative to upstream [NectarLink](https://github.com/thtbee/nectarlink) `v0.1.6-nightly.10` (`f6b8baf`).

## Local API

Upstream already provided a local HTTP API for listing devices, reading history and the clipboard, sending data and notifications, opening the phone camera, and starting playback on the phone. This fork adds routes for the Noctalia panel and refines the behavior of several existing commands. The plugin talks to a running NectarLink instance; the app communicates with the phone. Pairing and the relevant Android permissions are still required.

### Routes

| Method and path | Change from upstream |
| --- | --- |
| `GET /v1/devices` | Adds `ringing` and `controls` to the existing device, connectivity, and battery data. These report the find-my-phone ring state, toggle values, and whether each control is available. |
| `GET /v1/notifications` | Lists current notifications with the source phone's ID. |
| `POST /v1/page` | Reads messages, calls, photos, apps, and Deck tiles. |
| `POST /v1/page-action` | Performs actions in those sections. Reading a page does not execute commands. |
| `GET /v1/sessions`, `POST /v1/session` | Reads, starts, and stops sessions for the selected phone. |
| `POST /v1/type` | Sends text from the PC to the phone through an already enabled remote keyboard session. |
| `POST /v1/phone-toggle` | Changes phone toggle settings. |
| `POST /v1/ring` | Can stop as well as start the find-my-phone ring, and reports send failures to the client. |
| `POST /v1/camera` | Passes the exact ID of the selected phone instead of falling back to index `0`. |
| `POST /v1/clipboard`, `POST /v1/send`, `POST /v1/notify` | Rejects an invalid explicit target before changing the PC clipboard or sending data. |
| `GET /v1/clipboard`, `GET /v1/timeline`, `POST /v1/play-on-phone` | Keeps the existing routes and moves their handlers into separate modules. |

### Server, access, and phone selection

- [`local_api.rs`](desktop/app/src/local_api.rs) still starts and stops the server, applies settings, and defines the routing table. HTTP, token handling, and route handlers live in [`local_api/`](desktop/app/src/local_api/). The API still binds to `127.0.0.1` (port `53320` by default), not to an external network interface.
- [`http.rs`](desktop/app/src/local_api/http.rs) parses requests, rejects requests with an `Origin` header with `403`, and checks `Authorization: Bearer …`. The single five-second deadline is replaced by **5 seconds to read, up to 21 seconds to handle the route, and 5 seconds to write the response**. Page reads and actions have their own 20-second deadline so the server has time to return JSON instead of dropping the connection. When a route exceeds its deadline, the server attempts to return `504` with `error: "timeout"`.
- [`auth.rs`](desktop/app/src/local_api/auth.rs) retains the existing access mechanism: the port and token are stored in the app profile's private `api-token.json`. The file is replaced atomically. The current token survives a port change and is removed when the API is disabled.
- [`context.rs`](desktop/app/src/local_api/context.rs) accepts a known full ID or, on routes that support it, a unique match by ID prefix or partial name. An empty, unknown, or ambiguous explicit target **is not replaced with another phone**. When `device` is omitted, the first online phone is selected, or the first phone in the list if none are online. The new page and session routes accept full string IDs; the ID format in the phone protocol is unchanged.

<details>
<summary>Server startup, HTTP, and token storage</summary>

- `init()` applies the loaded settings and `sync_from_settings()` rereads `desktop-settings.json`. `apply_settings()` substitutes the default port when the configured port is zero. The server is not restarted if it is already listening on that port and the token is readable. Disabling the API stops the server and removes the token file. Token setup and bind failures are logged.
- `ServerHandle::stop()` signals the server to stop accepting new connections. Dropping `ServerHandle` calls the same method. The standalone `stop()` function takes down the running server but does not itself delete the token.
- `start_on_port()` handles connections independently. With test port `0`, the OS assigns the port. `read_http_request()` extracts the method, path, headers, and body; headers have a 256 KiB threshold, and the body is read according to `Content-Length` with a 256 KiB limit. `parse_target()` separates query parameters. `percent_decode()` decodes `%XX`, converts `+` to a space, and preserves malformed percent sequences. JSON responses include `Cache-Control: no-store`, and the connection is closed. Unknown method/path combinations return `404 not_found`.
- `generate_token()` gets 32 random bytes from the system and encodes them as 64 hexadecimal characters. It does not create a token if randomness is unavailable. `read_token_file()` requires a nonzero port and a valid token format; `write_token_file()` writes to a private temporary file and renames it. `constant_time_eq()` does not stop at the first differing character. The file does not contain pairing keys.
- `allowed()` checks that the phone is online and the feature is fully or partially available. `visible()` excludes apps marked `Hidden` when handling notification-based chats. The test helpers `with_state()` and `try_node()` do not connect to a real profile.

</details>

### Pages, actions, caching, and thumbnails

[`protocol.rs`](desktop/app/src/local_api/protocol.rs) defines two separate requests: a read with list parameters (`Query`), and an action with a `request_id` and confirmation (`Action`). Unknown fields are rejected. Depending on the section, reads support search or pagination; `more` indicates that additional results are available.

[`pages.rs`](desktop/app/src/local_api/pages.rs) validates request bounds, checks that the action targets the right phone, and verifies that the feature is available. `page-action` accepts only supported actions with `confirm: true`. The server runs at most one action on this route at a time. A request rejected as `busy` does not consume its ID. Once accepted, the ID is recorded even on error or timeout because the command may already have been sent. After 10,000 accepted IDs, new actions are rejected with `request_limit_restart_required` until NectarLink restarts.

<details>
<summary>Page route fields, limits, and responses</summary>

- **Read:** `device` and `page` are required. `thread` selects a conversation, `album` selects an album, `before` and `before_id` set the pagination boundary, `offset` and `limit` set the offset and page size, `search` filters results, and `force` requests a refresh. The maximum offset is 5,000 and the page size is 1–96. `thread` is limited to 1,024 bytes; `album`, `before_id`, and `search` are limited to 256 bytes each. `read_inner()` selects the handler; Deck can be read without a phone.
- **Action:** `device`, `action`, `request_id`, and `confirm` are required. `request_id` consists of 16–128 ASCII letters, digits, or hyphens. `id` is limited to 1,024 bytes, `text` to 4,096 bytes, and `recipients` to ten entries. Sending SMS requires nonempty text and recipients; a reply requires an ID prefixed with `chat:` and nonempty text; dialing requires a valid number format; other actions require an item ID. Before execution, `act_inner()` checks the capability on the selected phone, except for local Deck tiles.
- Read errors distinguish `invalid_json`, `invalid_page`, `unavailable`, and `timeout`. Action errors include `invalid_action`, `duplicate_request`, `busy`, `action_failed_check_phone`, and `uncertain_check_phone`. A rejection from `pages.rs` returns JSON with status `409`; the outer HTTP deadline returns a separate `504`. A failed response does not always mean the command was never sent.

</details>

| Section | What the API reads and does |
| --- | --- |
| [Messages](desktop/app/src/local_api/messages.rs) | Combines SMS threads with chats from visible notifications. Each chat includes up to 100 recent messages saved from notifications, not the full conversation history. If allowed, the API can also send SMS or reply through an active notification. |
| [Calls](desktop/app/src/local_api/calls.rs) | Reads call history when authorized and reports the active call. Can dial, answer, reject, or silence a call. An active-call command must match the call ID so it cannot affect the next call. |
| [Photos](desktop/app/src/local_api/photos.rs) | Reads the gallery with an album filter and a date/ID boundary. Also requests albums and thumbnails; their absence does not hide the photo list. Requesting an original returns a transfer ID, not the file itself. |
| [Apps](desktop/app/src/local_api/apps.rs) | Searches by display name or package name and returns paginated results and icon paths. Rechecks the exact package name on the phone before opening it. `request_only` means that opening was requested, not that a window is ready. |
| [Deck](desktop/app/src/local_api/deck.rs) | Reads up to 192 locally configured tiles: ID, label, page, and action type, with no command parameters. Runs an existing tile by ID; the API does not accept arbitrary commands. |

SMS threads use a `before` boundary. SMS and call history each request one extra entry to determine `more`: the default is five entries, with respective limits of seven and six. Photos are fetched nine at a time to return at most eight; albums and thumbnails are fetched in parallel. Apps default to 12 entries with a maximum of 96, and search is case-insensitive. Deck is read in one response with a `complete` flag and no pagination. Chats from visible notifications can still be available if SMS is unavailable or fails to load.

- [`cache.rs`](desktop/app/src/local_api/cache.rs) caches lists for at most two phones per read type: SMS threads for up to 20 seconds and apps for up to 120 seconds. `force` bypasses stale results. Errors, commands, and session state are not cached here.
- [`previews.rs`](desktop/app/src/local_api/previews.rs) stores thumbnails and icons in the private `noctalia-photo-previews` directory in the profile. Identical content reuses the file by hash. The first read removes old cache files, and new batches have limits on image count and size.

A concurrent forced read may reuse a result fetched after that read began. The cache lock is held while loading a list, so a slow response from one phone can delay the same kind of read for another. Before storing a new thumbnail batch, the tracked file count is trimmed to 288. The next batch is limited to 96 images of up to 128 KiB each. If an image cannot be written, its path is omitted from the response.

### Phone controls and sharing

- [`devices.rs`](desktop/app/src/local_api/devices.rs) still starts the find-my-phone ring when `on` is omitted and accepts `on: false` to stop it. With no available target or core it returns `409 no_device`; a send failure returns `502 ring_failed`.
- In the same module, `phone-toggle` checks the full ID, connectivity, and command availability on the target phone. It controls ringer mode, Do Not Disturb, flashlight, Wi-Fi, Bluetooth, and hotspot. Turning off Wi-Fi requires separate confirmation because it may disconnect the phone.
- [`notifications.rs`](desktop/app/src/local_api/notifications.rs) returns up to 100 current notifications from paired phones, excluding apps hidden by NectarLink rules. The response includes a phone ID so the panel can filter the list. `timeline` still reads the first page of history separately from current notifications.
- [`clipboard.rs`](desktop/app/src/local_api/clipboard.rs) reads PC clipboard text and up to 20 history entries. On writes, it checks any explicit phone target first: a `409 no_device` response leaves the PC clipboard unchanged. After a successful local write, NectarLink forwards the text if a target and the core are available. Success confirms the PC write, not receipt by the phone.
- [`transfers.rs`](desktop/app/src/local_api/transfers.rs) still sends files, links, and text in a single request. For `send` and `notify`, an invalid explicit recipient is rejected before sending. Without `device`, the existing behavior applies: `send` with no selected phone sends nothing, and `notify` may delegate recipient selection to the core. `ok: true` on these routes does not confirm delivery.

For `phone-toggle`, ringer mode accepts `mode: ring`, `vibrate`, or `silent`; all other toggles accept a Boolean `on`. Flashlight control requires both permission and a reported state from the phone. Successfully enabling the hotspot invokes the existing hotspot handling. `timeline` accepts a search query `q` and a `limit` (20 by default). `notify` requires a nonempty title and includes body text if provided. Clipboard history includes content kind, preview, source, direction, time, and pin status. Without the core, the history is empty.

### Sessions, audio, camera, and input

[`sessions.rs`](desktop/app/src/local_api/sessions.rs) reports the state and start availability of each session for each phone. A change request supplies `device`, `kind`, and `on`. Starting a session requires a connected phone that supports the feature. A stop request targets the known owner of the session and does not require it to remain online.

| Session | Purpose |
| --- | --- |
| `pc_audio` | Streams PC audio to the phone. An active stream is not switched to another device. |
| `watch` | Plays the current PC video on the selected phone. A suitable player is required. |
| `phone_audio` | Plays phone audio on the PC without opening the phone's screen. |
| `screen` | Opens a window with the selected phone's screen on the PC. |
| `webcam` | Uses the selected phone as a webcam. |
| `keyboard` | Enables remote typing from the PC to the phone. An active session for another phone is not switched. |

`set()` handles session commands one at a time. For shared resources, it returns `another_device` if the action would affect another phone's session. `pc_audio` uses the existing Deck action and settings persistence. `screen` does not open a second window if it is already waiting for permission or mirroring. `phone_audio` and `webcam` account for both active sessions and pending permission requests. The existing `POST /v1/play-on-phone` uses the standard phone selection and chooses the specified player, a playing one, or the first one. `nothing_playing` and `no_phone` distinguish a missing player from a missing phone.

- [`phone_sound.rs`](desktop/app/src/phone_sound.rs) now reports the target for both active audio and requests still awaiting confirmation. The audio pipeline is unchanged.
- [`play_on_phone.rs`](desktop/app/src/play_on_phone.rs) and [`play_on_phone/request.rs`](desktop/app/src/play_on_phone/request.rs) track the phone and the stage: `preparing` → `asking` → `watching`. Request numbering prevents an older preparation from overwriting newer state. Stopping cancels local preparation or playback; an offer already sent to the phone is not withdrawn. [`pc_mirror.rs`](desktop/app/src/pc_mirror.rs) distinguishes stopping playback from closing a regular screen mirroring session for the same phone.
- `POST /v1/camera` accepts `photo` or `scan`. [`continuity_camera.rs`](desktop/app/src/continuity_camera.rs) receives the exact ID; it does not substitute another online phone. The API returns `409 no_device` when the target is not found and `409 camera_unavailable` if the selected device disappears or the core is unavailable.
- `POST /v1/type` requires an enabled remote keyboard session for the specified phone and accepts nonempty text up to 16 KiB. [`mirror.rs`](desktop/app/src/mirror.rs) uses the shared input queue, splits text without breaking UTF-8 characters, and waits for each chunk's send result. It stops sending chunks if the target changes or a send fails. Disconnecting the HTTP request does not withdraw already queued input.

For sessions, `accepted` means that the request was accepted. For typing, `transport_only` means the text was sent by the transport, not acknowledged by Android. If an action times out, `uncertain_check_phone` means the result should be checked on the phone or PC. The plugin does not automatically retry these commands.

<details>
<summary>Input queue and Play on phone state</summary>

- `input_queue()` starts the shared handler on first use. Ordinary input queues events without waiting. For API input, the queue item carries a result channel. `run_input_queue()` reports errors to the waiting client but keeps the queue running if the client has already disconnected.
- `keyboard_text()` and `keyboard_text_checked()` share `checked_text_chunks()`. `send_checked_text()` checks the active target before starting and before each subsequent chunk. A failure does not undo chunks already sent; the response does not guarantee the text appeared on Android.
- `state_for()` only reports the selected phone's state. `play_for()` checks that it is online, and `stop_for()` does not stop another phone's session. `play_to()` marks the request as preparing and starts a separate thread. `Preparing::drop()` clears only unfinished preparation for its own request.
- `prepare()` collects the player, playback position, local file, and tab URL when available. `execute()` offers the file at its current position, a link, or a stream. If playback is unsupported but a fallback link exists, the link is sent. After waiting on the network, the code checks whether the request is still current before changing state or pausing the player.
- In `play_on_phone/request.rs`, `State::begin()` assigns a request number and records the target, and `State::asking()` records the pending state only for the current request. `State::finish_preparing()` does not clear a request that is already awaiting confirmation. `State::observed_watching()` clears the pending state only for the matching phone. `State::cancel()` changes the request number to invalidate old work. The pending state is displayed for up to 60 seconds; this timer does not revoke the offer on the phone.
- In `pc_mirror.rs`, `stop_matching()` checks the owner and stream type under the same lock. Stopping playback must not close a regular mirroring session that may have replaced it in the meantime.

</details>

## Noctalia palette

- [`external_palette.cpp`](desktop/app/cpp/external_palette.cpp) reads an existing `noctalia-colors.json` in the Qt configuration directory, normally `~/.config/nectarlink/`. It validates the file size (up to 64 KiB), the `light` or `dark` mode, and required roles with `#RRGGBB` values. It watches both the file and the directory, picking up new, replaced, or removed files without restarting. NectarLink does not create or export the palette.
- [`Theme.qml`](desktop/app/qml/Theme.qml) applies external colors and mode only to **Hive**. The animation is unchanged. The built-in colors are used when the switch is off or the file is missing or invalid. The plugin's own palette does not depend on this file.
- [`SettingsPage.qml`](desktop/app/qml/pages/SettingsPage.qml) adds a **Use Noctalia colors** switch for Hive. [`prefs.rs`](desktop/app/src/bridge/prefs.rs) and [`settings.rs`](desktop/app/src/settings.rs) save the explicit choice as `noctaliaColors`. The legacy `noctalia-colors.json.disabled` marker is consulted only until the user makes an explicit choice.
- [`build.rs`](desktop/app/build.rs) includes the loader and its [header](desktop/app/cpp/external_palette.h) in the build, and [`app_helpers.cpp`](desktop/app/cpp/app_helpers.cpp) registers it before QML loads. [`MainWindow.qml`](desktop/app/qml/MainWindow.qml) sets the main window title to `NectarLink — Noctalia Integration` without changing window behavior.

`PaletteLoader` reads the file immediately, then batches file and directory change notifications into a single reload after 60 ms. It exposes `valid`, `dark`, and `colors` to QML. With an external palette, Hive uses its primary color for the glow and AMOLED does not override external colors. Without an explicit choice, the setting follows file presence and the absence of the legacy marker; these checks do not replace JSON validation. A saved choice takes precedence, and reading the setting does not write the marker.

<details>
<summary>Palette file format</summary>

The JSON root contains `mode` (`light` or `dark`) and a `colors` object. `colors` requires the following keys with `#RRGGBB` string values:

```text
primary, onPrimary, primaryContainer, onPrimaryContainer,
secondaryContainer, onSecondaryContainer,
surface, surfaceContainerLow, surfaceContainer, surfaceContainerHigh,
surfaceContainerHighest, onSurface, onSurfaceVariant, outline,
outlineVariant, warning, error, onError
```

</details>

## Updates, version, and packaging

- On Linux, [`updater.rs`](desktop/app/src/updater.rs) disables background and manual checks of the upstream update channel, update installation, and startup of the update service. An upstream package could otherwise replace the fork and remove the integration. Replace fork builds manually; the profile is stored separately.
- [`Cargo.toml`](Cargo.toml) and [`Cargo.lock`](Cargo.lock) use `0.1.5+noctalia.2` for the build and local dependencies. `v0.1.6-nightly.10` identifies the upstream source snapshot, not the binary version.
- The [**Build** workflow](.github/workflows/build.yml) can be run manually without a tag or publication. It builds only Linux packages on Ubuntu 24.04 and uploads `.deb`, `.rpm`, AppImage, and tarball files in the `linux` artifact. A manual run uses the source version; `version` can be passed when called from another workflow. Cargo runs with `--locked`.
- For a `v…+noctalia.…` tag, the [**Release** workflow](.github/workflows/release.yml) checks it against the version in `Cargo.toml`, builds and validates Linux packages only, and attaches them, `install.sh`, and `SHA256SUMS.txt` to a draft release. Asset names replace `+` with `-`; the tag and version inside the packages are unchanged. Publishing this release does not update the upstream auto-update channel.
- [`stage-translations.sh`](desktop/packaging/linux/stage-translations.sh) prepares `.qm` catalogs for AppImage, `.deb`, and tarball packaging. The `.rpm` inherits them from the `.deb`. Missing or outdated catalogs are compiled from `.ts` files; packaging stops if `nectarlink_ru.qm` is missing. Russian labels for the palette switch live in `nectarlink_ru.ts`, not in the QML source strings.
- [`install.sh`](desktop/packaging/linux/install.sh) looks for the release, packages, and `SHA256SUMS.txt` only in `vladikchi/nectarlink-noctalia`. It keeps the existing selection between `.deb`, `.rpm`, and tarball. If the fork's release is unavailable, the installer does not fall back to upstream. The upstream Android app is linked separately.

To prepare translations, the script uses `LRELEASE` or looks for `lrelease` via `QMAKE` and `PATH`. It reuses up-to-date, nonempty catalogs. The AppImage includes Qt, the required QML modules, and prepared FFmpeg video adapters, but does not replace the system portal, multimedia components, or capture drivers. Manual workflow artifacts are available from the successful Actions run; local changes not on GitHub are not included.

## Verification and build scope

- The app passed `cargo check` and 20 local API tests. Coverage includes authorization and routing, invalid explicit targets, ID formats, confirmation rules, and rejection of repeated actions. Palette coverage includes a separate [loader test](desktop/app/tests/external_palette_test.cpp) and tests for persisting the setting in [`settings.rs`](desktop/app/src/settings.rs).
- A local `Nectarlink-0.1.5+noctalia.1-x86_64.AppImage` was built on Arch with Qt 6.12.0 and glibc 2.44. It passed `--version` and `--ui-check` with a temporary profile and loaded the Russian translation. That check applied to the local package, not the Ubuntu-built release, and did not cover a phone, the Wayland portal, or other distributions.
- The installer URLs and rejection of a bad SHA-256 checksum were tested with mocked GitHub responses. Those tests do not replace installing from a public release.
- The Island, keyboard layout, and screen capture fixes for [Niri](https://github.com/vladikchi/nectarlink-niri) are not included in the Noctalia build.

<details>
<summary>Source test coverage</summary>

| Location | Coverage |
| --- | --- |
| `local_api.rs`, `http.rs`, `auth.rs`, `context.rs` | Token access, rejection of missing tokens and requests with `Origin`, unknown routes, invalid JSON, URL parameter parsing, port changes and token retention, string IDs, and rejection of unknown or ambiguous targets. |
| `pages.rs` | Cache lifetime, `force`, separate lists for two phones, gallery, app, and Deck bounds, action confirmation, and rejection of reused IDs. |
| `devices.rs`, `notifications.rs` | Toggle value schema, confirmation before disabling Wi-Fi, rejection of unsupported actions, starting and stopping the find-my-phone ring, and an empty notification list without the core. |
| `sessions.rs`, `mirror.rs` | Isolation from real sessions, valid IDs and command types, input order, stopping after an error or target change, and UTF-8 boundaries. |
| `play_on_phone/request.rs` | Late results do not change newer requests, cancellation is tied to a phone, and observing playback does not clear preparation for a different device. |
| `external_palette_test.cpp`, `settings.rs`, `tests/qml/stubs/Preferences.qml` | Missing, new, and replaced files, mode switching, invalid JSON and colors, file restoration and removal, migration of the legacy setting, and persistence of an explicit choice. The QML stub exposes `noctaliaColors` without using a real profile. |

</details>
