// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed, device-scoped session control. Start acknowledgements are NOT claims
//! that capture started or Android accepted its permission prompt.
use super::context::resolve_target_device;
use super::context::{display_device_id, try_node, with_state};
use nectarlink_core::{DeviceId, FeatureState, LinkState};
use serde::Deserialize;
use serde_json::{Value, json};

type Reply = (u16, &'static str, Value);
fn error(code: u16, reason: &'static str, key: &str) -> Reply {
    (code, reason, json!({"ok":false,"error":key}))
}
fn feature(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "pc_audio" => "media.pc_audio",
        "watch" => "media.play_on_phone",
        "phone_audio" => "mirroring.sound",
        "screen" => "mirroring.view",
        "webcam" => "camera.webcam",
        "keyboard" => "mirroring.control",
        _ => return None,
    })
}
fn permitted(s: &crate::state::AppState, device: DeviceId, kind: &str) -> bool {
    feature(kind).is_some_and(|id| {
        matches!(
            s.matrices.get(&device).and_then(|m| m.state(id)),
            Some(FeatureState::Available | FeatureState::Partial { .. })
        )
    })
}

#[cfg(not(test))]
fn pc_audio_target() -> Option<DeviceId> {
    crate::pc_audio_stream::target().or_else(|| {
        with_state(|s| s.devices.iter().find(|d| matches!(d.link, LinkState::Online { .. })).map(|d| d.id))
            .flatten()
    })
}

pub(super) fn get() -> Reply {
    #[cfg(test)]
    return (200, "OK", json!({"ok":true,"devices":[]}));
    #[cfg(not(test))]
    {
        // Release the hub read lock before querying modules that also read it.
        let devices = with_state(|s| {
            s.devices
                .iter()
                .map(|d| {
                    let mut allowed = serde_json::Map::new();
                    for kind in ["pc_audio", "watch", "phone_audio", "screen", "webcam", "keyboard"] {
                        allowed.insert(
                            kind.into(),
                            json!(matches!(d.link, LinkState::Online { .. }) && permitted(s, d.id, kind)),
                        );
                    }
                    (d.id, allowed)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
        let target = pc_audio_target();
        let watchable = crate::play_on_phone::watchable().is_some();
        let webcam = crate::webcam::phase();
        let devices = devices.into_iter().map(|(id, mut allowed)| {
            let watch = crate::play_on_phone::state_for(id);
            if !watchable && watch == "off" { allowed.insert("watch".into(), json!(false)); }
            let screen = match crate::mirror::phase(&crate::mirror::Window::screen(id)) {
                Some(crate::mirror::Phase::Asking) => "asking",
                Some(crate::mirror::Phase::Showing) => "showing",
                Some(crate::mirror::Phase::Ended(_)) => "ended", None => "off",
            };
            let cam = match &webcam {
                crate::webcam::Phase::Asking{device} if *device == id => "asking",
                crate::webcam::Phase::Streaming{device,..} if *device == id => "streaming", _ => "off",
            };
            json!({"id":id.to_string(),"allowed":allowed,
                "pc_audio":if crate::pc_audio_stream::is_enabled() && target == Some(id) {"enabled"} else {"off"},
                "watch":watch,"phone_audio":crate::phone_sound::state(&id),"screen":screen,"webcam":cam,
                "keyboard":if crate::mirror::keyboard_device()==Some(id){"enabled"}else{"off"}})
        }).collect::<Vec<_>>();
        (200, "OK", json!({"ok":true,"devices":devices}))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    #[serde(deserialize_with = "display_device_id")]
    device: DeviceId,
    kind: String,
    on: bool,
}
static COMMAND: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) async fn set(body: &[u8]) -> Reply {
    let p: Payload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(_) => return error(400, "Bad Request", "invalid_json"),
    };
    if feature(&p.kind).is_none() {
        return error(400, "Bad Request", "invalid_session");
    }
    let _command = COMMAND.lock().await;
    let info = with_state(|s| {
        s.devices
            .iter()
            .find(|d| d.id == p.device)
            .map(|d| (matches!(d.link, LinkState::Online { .. }), permitted(s, p.device, &p.kind)))
    })
    .flatten();
    let Some((online, allowed)) = info else { return error(409, "Conflict", "no_device") };
    if p.on && (!online || !allowed) {
        return error(409, "Conflict", "unavailable");
    }
    if try_node().is_none() {
        return error(409, "Conflict", "no_device");
    }
    #[cfg(test)]
    return error(409, "Conflict", "no_device");
    #[cfg(not(test))]
    {
        use crate::{mirror, pc_audio_stream, phone_sound, play_on_phone, webcam};
        match p.kind.as_str() {
            "pc_audio" => {
                if pc_audio_stream::is_enabled() && pc_audio_target().is_some_and(|d| d != p.device) {
                    return error(409, "Conflict", "another_device");
                }
                // Reuse the native Deck action so Preferences and persistence
                // follow exactly the same path as commands from the phone.
                let level = if p.on {
                    nectarlink_core::deck::PHONE_SPEAKER_ON
                } else {
                    nectarlink_core::deck::PHONE_SPEAKER_OFF
                };
                if crate::deck::handle_set_audio(&p.device, Some(level), None).is_err() {
                    return error(502, "Bad Gateway", "session_failed");
                }
            }
            "watch" => {
                if play_on_phone::target().is_some_and(|d| d != p.device) {
                    return error(409, "Conflict", "another_device");
                }
                if p.on {
                    let Some(player) = play_on_phone::watchable() else {
                        return error(409, "Conflict", "nothing_playing");
                    };
                    if play_on_phone::state_for(p.device) == "off" {
                        match play_on_phone::play_for(p.device, player) {
                            Ok(()) => {}
                            Err(crate::play_on_phone::StartError::NoDevice) => {
                                return error(409, "Conflict", "no_device");
                            }
                            Err(crate::play_on_phone::StartError::Spawn(_)) => {
                                return error(502, "Bad Gateway", "session_failed");
                            }
                        }
                    }
                } else if play_on_phone::state_for(p.device) != "off" {
                    if let Err(reason) = play_on_phone::stop_for(p.device) {
                        return error(409, "Conflict", reason);
                    }
                }
            }
            "phone_audio" => {
                if phone_sound::target().is_some_and(|d| d != p.device) {
                    return error(409, "Conflict", "another_device");
                }
                phone_sound::set_on(p.device, p.on);
            }
            "screen" => {
                let window = mirror::Window::screen(p.device);
                if p.on {
                    if !matches!(mirror::phase(&window), Some(mirror::Phase::Asking | mirror::Phase::Showing))
                    {
                        mirror::start(p.device);
                    }
                } else if matches!(
                    mirror::phase(&window),
                    Some(mirror::Phase::Asking | mirror::Phase::Showing)
                ) {
                    mirror::stop(window);
                }
            }
            "webcam" => match webcam::phase() {
                webcam::Phase::Asking { device } | webcam::Phase::Streaming { device, .. }
                    if device != p.device =>
                {
                    return error(409, "Conflict", "another_device");
                }
                webcam::Phase::Asking { .. } | webcam::Phase::Streaming { .. } => {
                    if !p.on {
                        webcam::stop();
                    }
                }
                _ => {
                    if p.on {
                        webcam::start(Some(p.device));
                    }
                }
            },
            "keyboard" => {
                if mirror::keyboard_device().is_some_and(|d| d != p.device) {
                    return error(409, "Conflict", "another_device");
                }
                mirror::set_remote_keyboard(p.on.then_some(p.device));
            }
            _ => unreachable!(),
        }
        (200, "OK", json!({"ok":true,"accepted":true}))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TypePayload {
    #[serde(deserialize_with = "display_device_id")]
    device: DeviceId,
    text: String,
}
static TYPE_COMMAND: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub(super) async fn type_text(body: &[u8]) -> Reply {
    let p: TypePayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(_) => return error(400, "Bad Request", "invalid_json"),
    };
    if p.text.is_empty() || p.text.len() > 16 * 1024 {
        return error(400, "Bad Request", "invalid_text");
    }
    let _command = TYPE_COMMAND.lock().await;
    let allowed = with_state(|s| {
        s.devices.iter().any(|d| d.id == p.device && matches!(d.link, LinkState::Online { .. }))
            && permitted(s, p.device, "keyboard")
    })
    .unwrap_or(false);
    if !allowed || try_node().is_none() {
        return error(409, "Conflict", "unavailable");
    }
    #[cfg(not(test))]
    {
        if crate::mirror::keyboard_device() != Some(p.device) {
            return error(409, "Conflict", "keyboard_off");
        }
        if crate::mirror::keyboard_text_checked(p.device, &p.text).await.is_err() {
            return (
                502,
                "Bad Gateway",
                json!({"ok":false,"error":"input_failed","may_have_been_sent":true}),
            );
        }
        return (200, "OK", json!({"ok":true,"sent":true,"confirmation":"transport_only"}));
    }
    #[cfg(test)]
    error(409, "Conflict", "unavailable")
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct PlayOnPhonePayload {
    /// The PC player's ID; by default the one playing (else the first).
    #[serde(default)]
    player: Option<String>,
}

/// Play on phone: what a player of this PC plays, on the phone.
pub(super) fn handle_post_play_on_phone(body: &[u8]) -> (u16, &'static str, Value) {
    let payload: PlayOnPhonePayload = if body.is_empty() {
        PlayOnPhonePayload::default()
    } else {
        match serde_json::from_slice(body) {
            Ok(p) => p,
            Err(_) => return (400, "Bad Request", json!({ "ok": false, "error": "invalid_json" })),
        }
    };
    let players = crate::media::local_views();
    let player = match payload.player {
        Some(id) => players.iter().find(|v| v.player.id == id),
        None => players.iter().find(|v| v.player.playing).or_else(|| players.first()),
    };
    let Some(player) = player.map(|v| v.player.id.clone()) else {
        return (404, "Not Found", json!({ "ok": false, "error": "nothing_playing" }));
    };
    if crate::play_on_phone::phone().is_none() {
        return (409, "Conflict", json!({ "ok": false, "error": "no_phone" }));
    }
    crate::play_on_phone::play(player.clone());
    (200, "OK", json!({ "ok": true, "player": player }))
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct CameraPayload {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    device: Option<String>,
}

pub(super) fn handle_post_camera(body: &[u8]) -> (u16, &'static str, Value) {
    let payload: CameraPayload = if body.is_empty() {
        CameraPayload::default()
    } else {
        match serde_json::from_slice(body) {
            Ok(p) => p,
            Err(_) => return (400, "Bad Request", json!({ "ok": false, "error": "invalid_json" })),
        }
    };
    let mode = match payload.mode.as_deref().map(str::trim) {
        Some("scan") => "scan",
        Some("photo") | None | Some("") => "photo",
        Some(_) => return (400, "Bad Request", json!({ "ok": false, "error": "invalid_mode" })),
    };
    let Some(device) = resolve_target_device(payload.device.as_deref()) else {
        return error(409, "Conflict", "no_device");
    };
    #[cfg(not(test))]
    if !crate::continuity_camera::start_for_device(device, mode) {
        return error(409, "Conflict", "camera_unavailable");
    }
    #[cfg(test)]
    let _ = device;
    (200, "OK", json!({ "ok": true, "mode": mode }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_is_empty_without_core_and_payloads_are_strict() {
        assert_eq!(get().2["devices"], json!([]));
        assert!(feature("install_addon").is_none());
        assert!(
            serde_json::from_value::<Payload>(
                json!({"device":DeviceId([0;32]).to_string(),"kind":"screen","on":"true"})
            )
            .is_err()
        );
    }
    #[test]
    fn http_payloads_use_the_same_display_id_strings_as_the_plugin() {
        let id = DeviceId([7; 32]);
        for kind in ["pc_audio", "watch", "phone_audio", "screen", "webcam", "keyboard"] {
            let raw = json!({"device":id.to_string(),"kind":kind,"on":true}).to_string();
            let parsed: Payload = serde_json::from_slice(raw.as_bytes()).unwrap();
            assert_eq!(parsed.device, id);
            assert_eq!(parsed.kind, kind);
            assert!(parsed.on);
        }
        let raw = json!({"device":id.to_string(),"text":"Проверка 🦀"}).to_string();
        let parsed: TypePayload = serde_json::from_slice(raw.as_bytes()).unwrap();
        assert_eq!(parsed.device, id);
        assert_eq!(parsed.text, "Проверка 🦀");
        assert!(serde_json::from_value::<Payload>(json!({"device":id,"kind":"screen","on":true})).is_err());
        assert!(serde_json::from_value::<TypePayload>(json!({"device":"invalid","text":"test"})).is_err());
    }
    #[tokio::test]
    async fn no_real_sessions_are_started_in_tests() {
        for kind in ["pc_audio", "watch", "phone_audio", "screen", "webcam", "keyboard"] {
            for on in [false, true] {
                let p = json!({"device":DeviceId([0;32]).to_string(),"kind":kind,"on":on});
                assert_eq!(set(p.to_string().as_bytes()).await.0, 409);
            }
        }
        assert_eq!(set(b"{}").await.0, 400);
        assert_eq!(
            type_text(json!({"device":DeviceId([0;32]).to_string(),"text":"test"}).to_string().as_bytes())
                .await
                .0,
            409
        );
    }
}
