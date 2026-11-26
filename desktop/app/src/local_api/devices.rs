// SPDX-License-Identifier: GPL-3.0-or-later
use super::context::{feature_access, resolve_target_device, try_node, with_state};
use nectarlink_core::{Battery, DeviceId, LinkState};
use serde::Deserialize;
use serde_json::{Value, json};
pub(super) fn handle_get_devices() -> (u16, &'static str, Value) {
    let privacy = super::private_epoch();
    let (ready, devices, policy) = with_state(|s| {
        let devices = s
            .devices
            .iter()
            .map(|d| {
                let (online, link_kind) = match &d.link {
                    LinkState::Online { path, .. } => (
                        true,
                        match path {
                            nectarlink_core::ConnectionPath::Lan => "lan",
                            nectarlink_core::ConnectionPath::Usb => "usb",
                            nectarlink_core::ConnectionPath::Relay => "relay",
                            nectarlink_core::ConnectionPath::Bluetooth => "bluetooth",
                        },
                    ),
                    LinkState::Connecting => (false, "connecting"),
                    LinkState::Offline { .. } => (false, "offline"),
                };
                let battery = d.battery.as_ref().map(battery_json);
                json!({
                    "id": d.id.to_string(),
                    "name": d.info.name,
                    "platform": d.info.os,
                    "online": online,
                    "link": link_kind,
                    "battery": battery,
                    "smsAllowed": feature_access(s, d.id, "messages.sms"),
                    "appWindowsAllowed": feature_access(s, d.id, "mirroring.app_windows"),
                    "ringing": s.ringing_devices.contains(&d.id),
                    "controls": phone_controls_json(s, d.id),
                })
            })
            .collect::<Vec<_>>();
        (s.paired_devices_ready, devices, privacy.and_then(|_| super::privacy::policy(s)))
    })
    .unwrap_or_default();
    let privacy = privacy.filter(|&epoch| super::private_epoch() == Some(epoch));

    (
        200,
        "OK",
        json!({
            "ok": true, "devices": devices, "devicesReady": ready,
            "localMessageThreads": true, "pageCacheV2": true,
            "privateMessages": privacy.is_some(),
            "privateReadEpoch": privacy.map(|epoch| epoch.to_string()),
            "notificationPolicy": privacy.and(policy),
        }),
    )
}

fn battery_json(battery: &Battery) -> Value {
    json!({
        "level": battery.level,
        "charging": battery.charging,
        "fullInMinutes": battery.full_in.filter(|&minutes| battery.charging && minutes > 0),
    })
}

#[derive(Debug, Deserialize)]
struct RingPayload {
    #[serde(default)]
    device: Option<String>,
    #[serde(default = "ring_default_on")]
    on: bool,
}

fn ring_default_on() -> bool {
    true
}

impl Default for RingPayload {
    fn default() -> Self {
        Self { device: None, on: true }
    }
}

pub(super) async fn handle_post_ring(body: &[u8]) -> (u16, &'static str, Value) {
    let payload: RingPayload = if body.is_empty() {
        RingPayload::default()
    } else {
        match serde_json::from_slice(body) {
            Ok(p) => p,
            Err(_) => return (400, "Bad Request", json!({ "ok": false, "error": "invalid_json" })),
        }
    };
    if let (Some(device), Some(node)) = (resolve_target_device(payload.device.as_deref()), try_node()) {
        if node.ring(device, payload.on).await.is_err() {
            return (502, "Bad Gateway", json!({ "ok": false, "error": "ring_failed" }));
        }
        return (200, "OK", json!({ "ok": true, "on": payload.on }));
    }
    (409, "Conflict", json!({ "ok": false, "error": "no_device" }))
}

fn toggle_feature(id: &str) -> Option<&'static str> {
    match id {
        "dnd" => Some("toggles.dnd"),
        "ringer" => Some("toggles.ringer"),
        "flashlight" => Some("toggles.flashlight"),
        "wifi" | "hotspot" => Some("toggles.wifi"),
        "bluetooth" => Some("toggles.bluetooth"),
        _ => None,
    }
}

fn toggle_allowed(s: &crate::state::AppState, device: DeviceId, id: &str) -> bool {
    use nectarlink_core::features::FeatureState;
    let Some(feature) = toggle_feature(id) else { return false };
    matches!(
        s.matrices.get(&device).and_then(|m| m.state(feature)),
        Some(FeatureState::Available | FeatureState::Partial { .. })
    ) && (id != "flashlight" || s.toggles.get(&device).is_some_and(|t| t.flashlight.is_some()))
}

fn phone_controls_json(s: &crate::state::AppState, device: DeviceId) -> Value {
    let mut allowed = serde_json::Map::new();
    for id in ["dnd", "ringer", "flashlight", "wifi", "bluetooth", "hotspot"] {
        allowed.insert(id.into(), json!(toggle_allowed(s, device, id)));
    }
    json!({ "state": s.toggles.get(&device), "allowed": allowed })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhoneTogglePayload {
    device: String,
    id: String,
    on: Option<bool>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    confirm_disconnect: bool,
}

fn phone_toggle_value(payload: &PhoneTogglePayload) -> Option<nectarlink_core::PhoneToggleValue> {
    use nectarlink_core::PhoneToggleValue;
    if payload.id == "ringer" {
        match (payload.on, payload.mode.as_deref()) {
            (None, Some(mode @ ("ring" | "vibrate" | "silent"))) => Some(PhoneToggleValue::Mode(mode.into())),
            _ => None,
        }
    } else if toggle_feature(&payload.id).is_some() && payload.mode.is_none() {
        payload.on.map(PhoneToggleValue::Bool)
    } else {
        None
    }
}

pub(super) async fn handle_post_phone_toggle(body: &[u8]) -> (u16, &'static str, Value) {
    let payload: PhoneTogglePayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(_) => return (400, "Bad Request", json!({"ok":false,"error":"invalid_json"})),
    };
    if toggle_feature(&payload.id).is_none() {
        return (400, "Bad Request", json!({"ok":false,"error":"invalid_toggle"}));
    }
    let Some(value) = phone_toggle_value(&payload) else {
        return (400, "Bad Request", json!({"ok":false,"error":"invalid_toggle_value"}));
    };
    let Ok(device) = payload.device.parse::<DeviceId>() else {
        return (400, "Bad Request", json!({"ok":false,"error":"invalid_device"}));
    };
    if payload.id == "wifi" && payload.on == Some(false) && !payload.confirm_disconnect {
        return (409, "Conflict", json!({"ok":false,"error":"confirmation_required"}));
    }
    let permitted = with_state(|s| {
        s.devices.iter().any(|d| d.id == device && matches!(d.link, LinkState::Online { .. }))
            && toggle_allowed(s, device, &payload.id)
    })
    .unwrap_or(false);
    if !permitted {
        return (409, "Conflict", json!({"ok":false,"error":"unavailable"}));
    }
    let Some(node) = try_node() else {
        return (409, "Conflict", json!({"ok":false,"error":"no_device"}));
    };
    match node.set_phone_toggle(device, payload.id.clone(), value).await {
        Ok(()) => {
            if payload.id == "hotspot" && payload.on == Some(true) {
                #[cfg(not(test))]
                crate::hotspot::hotspot_turned_on(device);
            }
            (200, "OK", json!({"ok":true}))
        }
        Err(_) => (502, "Bad Gateway", json!({"ok":false,"error":"toggle_failed"})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list_is_not_ready_until_the_first_successful_load() {
        let mut state = crate::state::AppState::default();
        assert!(!state.paired_devices_ready);
        state.set_devices(Vec::new());
        assert!(state.paired_devices_ready && state.devices.is_empty());
    }

    #[test]
    fn reports_only_a_positive_charge_estimate_while_charging() {
        let mut battery = Battery {
            level: 42,
            charging: true,
            full_in: Some(75),
            plugged: None,
            cell_bars: None,
            cell_type: None,
            wifi_bars: None,
        };
        assert_eq!(battery_json(&battery), json!({"level":42,"charging":true,"fullInMinutes":75}));
        for (charging, full_in) in [(false, Some(75)), (true, None), (true, Some(0))] {
            battery.charging = charging;
            battery.full_in = full_in;
            assert_eq!(battery_json(&battery), json!({"level":42,"charging":charging,"fullInMinutes":null}));
        }
    }

    #[test]
    fn phone_controls_have_no_permissions_without_a_feature_matrix() {
        let id = DeviceId([0; 32]);
        let s = crate::state::AppState::default();
        let controls = phone_controls_json(&s, id);
        assert!(controls["state"].is_null());
        for id in ["dnd", "flashlight", "wifi", "bluetooth", "hotspot"] {
            assert_eq!(controls["allowed"][id], false);
        }
        assert!(toggle_feature("remote_input").is_none());
        let mut allowed_state = crate::state::AppState::default();
        allowed_state.matrices.insert(
            id,
            nectarlink_core::features::CapabilityMatrix {
                device: id,
                features: std::collections::BTreeMap::from([
                    ("toggles.wifi", nectarlink_core::features::FeatureState::Available),
                    ("toggles.flashlight", nectarlink_core::features::FeatureState::Available),
                ]),
            },
        );
        assert!(toggle_allowed(&allowed_state, id, "wifi"));
        assert!(!toggle_allowed(&allowed_state, id, "flashlight")); // No reported flash hardware.
        assert!(
            serde_json::from_str::<PhoneTogglePayload>(r#"{"device":"x","id":"dnd","on":"true"}"#).is_err()
        );
    }
    #[tokio::test]
    async fn ringer_modes_are_strict_and_never_dispatch_without_capability() {
        let device = DeviceId([0; 32]).to_string();
        assert_eq!(toggle_feature("ringer"), Some("toggles.ringer"));
        for mode in ["ring", "vibrate", "silent"] {
            let body = json!({"device":device,"id":"ringer","mode":mode});
            let payload: PhoneTogglePayload = serde_json::from_value(body.clone()).unwrap();
            assert!(
                matches!(phone_toggle_value(&payload),Some(nectarlink_core::PhoneToggleValue::Mode(m)) if m==mode)
            );
            assert_eq!(handle_post_phone_toggle(body.to_string().as_bytes()).await.0, 409);
        }
        for body in [
            json!({"device":device,"id":"ringer","mode":"normal"}),
            json!({"device":device,"id":"ringer","mode":"ring","on":false}),
            json!({"device":device,"id":"ringer","on":true}),
            json!({"device":device,"id":"dnd","mode":"silent","on":true}),
            json!({"device":device,"id":"dnd"}),
        ] {
            assert_eq!(handle_post_phone_toggle(body.to_string().as_bytes()).await.0, 400);
        }
    }
    #[tokio::test]
    async fn phone_toggle_requires_valid_id_device_confirmation_and_online_capability() {
        assert_eq!(handle_post_phone_toggle(b"{}").await.0, 400);
        assert_eq!(handle_post_phone_toggle(br#"{"device":"bad","id":"dnd","on":true}"#).await.0, 400);
        let device = DeviceId([0; 32]).to_string();
        let off = json!({"device":device,"id":"wifi","on":false});
        let (status, _, result) = handle_post_phone_toggle(off.to_string().as_bytes()).await;
        assert_eq!(status, 409);
        assert_eq!(result["error"], "confirmation_required");
        let dnd = json!({"device":device,"id":"dnd","on":true});
        assert_eq!(handle_post_phone_toggle(dnd.to_string().as_bytes()).await.0, 409);
    }
    #[test]
    fn ring_payload_preserves_start_default_and_explicit_stop() {
        assert!(RingPayload::default().on);
        assert!(serde_json::from_str::<RingPayload>("{}").unwrap().on);
        let stop: RingPayload = serde_json::from_str(r#"{"device":"phone","on":false}"#).unwrap();
        assert!(!stop.on);
        assert_eq!(stop.device.as_deref(), Some("phone"));
        assert!(serde_json::from_str::<RingPayload>(r#"{"on":"false"}"#).is_err());
    }
    #[tokio::test]
    async fn ring_without_device_reports_failure_for_start_and_stop() {
        for body in [b"".as_slice(), b"{}", br#"{"on":false}"#] {
            let (code, _, data) = handle_post_ring(body).await;
            assert_eq!(code, 409);
            assert_eq!(data["ok"], false);
            assert_eq!(data["error"], "no_device");
        }
        assert_eq!(handle_post_ring(b"{broken").await.0, 400);
    }
}
