// SPDX-License-Identifier: GPL-3.0-or-later
use nectarlink_core::{DeviceId, FeatureState, LinkState};
use serde::{Deserialize, Deserializer};
#[cfg(not(test))]
pub(super) fn with_state<T>(f: impl FnOnce(&crate::state::AppState) -> T) -> Option<T> {
    Some(crate::core_host::host().hub.read(f))
}

#[cfg(test)]
pub(super) fn with_state<T>(_f: impl FnOnce(&crate::state::AppState) -> T) -> Option<T> {
    None
}

#[cfg(not(test))]
pub(super) fn try_node() -> Option<nectarlink_core::Node> {
    crate::core_host::node()
}

#[cfg(test)]
pub(super) fn try_node() -> Option<nectarlink_core::Node> {
    None
}

pub(super) fn resolve_target_device(requested: Option<&str>) -> Option<DeviceId> {
    let query = requested.map(str::trim);
    with_state(|s| {
        if let Some(q) = query {
            return match_device(q, s.devices.iter().map(|d| (d.id, d.info.name.as_str())));
        }
        s.devices
            .iter()
            .find(|d| matches!(d.link, LinkState::Online { .. }))
            .or_else(|| s.devices.first())
            .map(|d| d.id)
    })
    .flatten()
}

fn match_device<'a>(query: &str, devices: impl Iterator<Item = (DeviceId, &'a str)>) -> Option<DeviceId> {
    if query.is_empty() {
        return None;
    }
    let mut devices = devices;
    if let Ok(id) = query.parse::<DeviceId>() {
        return devices.find(|(candidate, _)| *candidate == id).map(|(id, _)| id);
    }
    let query = query.to_ascii_lowercase();
    let mut matches = devices.filter(|(id, name)| {
        id.to_string().starts_with(&query) || name.to_ascii_lowercase().contains(&query)
    });
    let (id, _) = matches.next()?;
    matches.next().is_none().then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_device_never_falls_back() {
        let first = DeviceId([1; 32]);
        let second = DeviceId([2; 32]);
        let resolve =
            |query: &str| match_device(query, [(first, "Work Phone"), (second, "Home Phone")].into_iter());
        assert_eq!(resolve(&second.to_string()), Some(second));
        assert_eq!(resolve("home"), Some(second));
        assert_eq!(resolve("phone"), None);
        assert_eq!(resolve("unknown"), None);
        assert_eq!(resolve(&DeviceId([3; 32]).to_string()), None);
        assert_eq!(resolve(""), None);
    }
}

// DeviceId's protocol serde uses 32 binary bytes. The HTTP interface uses the
// displayed z-base-32 string returned by /v1/devices and /v1/sessions instead.
// Do NOT change core DeviceId serde: that would break CBOR and stored identities.
pub(super) fn display_device_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DeviceId, D::Error> {
    String::deserialize(deserializer)?.parse().map_err(serde::de::Error::custom)
}
pub(super) fn online_with_capabilities(device: DeviceId) -> bool {
    with_state(|s| {
        s.matrices.contains_key(&device)
            && s.devices.iter().any(|d| d.id == device && matches!(d.link, LinkState::Online { .. }))
    })
    .unwrap_or(false)
}

pub(super) fn allowed(device: DeviceId, feature: &str) -> bool {
    with_state(|s| feature_access(s, device, feature)).flatten() == Some(true)
}

pub(super) fn feature_access(s: &crate::state::AppState, device: DeviceId, feature: &str) -> Option<bool> {
    online_feature_state(s, device, feature)
        .map(|state| matches!(state, FeatureState::Available | FeatureState::Partial { .. }))
}

pub(super) fn online_feature_state(
    s: &crate::state::AppState,
    device: DeviceId,
    feature: &str,
) -> Option<FeatureState> {
    if !s.devices.iter().any(|d| d.id == device && matches!(d.link, LinkState::Online { .. })) {
        return None;
    }
    s.matrices.get(&device)?.state(feature)
}

pub(super) fn sms_access(device: DeviceId) -> Option<bool> {
    with_state(|s| feature_access(s, device, "messages.sms")).flatten()
}
pub(super) fn visible(app: &str) -> bool {
    with_state(|s| s.app_rule(app) != crate::state::AppRule::Hidden).unwrap_or(false)
}
