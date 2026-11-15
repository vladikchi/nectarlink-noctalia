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
    let query = requested.map(str::trim).filter(|s| !s.is_empty());
    if let Some(q) = query
        && let Ok(id) = q.parse::<DeviceId>()
    {
        return Some(id);
    }
    with_state(|s| {
        if let Some(q) = query {
            let lower = q.to_ascii_lowercase();
            if let Some(found) = s.devices.iter().find(|d| {
                d.id.to_string().starts_with(&lower) || d.info.name.to_ascii_lowercase().contains(&lower)
            }) {
                return Some(found.id);
            }
        }
        s.devices
            .iter()
            .find(|d| matches!(d.link, LinkState::Online { .. }))
            .or_else(|| s.devices.first())
            .map(|d| d.id)
    })
    .flatten()
}

// DeviceId's protocol serde uses 32 binary bytes. The HTTP interface uses the
// displayed z-base-32 string returned by /v1/devices and /v1/sessions instead.
// Do NOT change core DeviceId serde: that would break CBOR and stored identities.
pub(super) fn display_device_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DeviceId, D::Error> {
    String::deserialize(deserializer)?.parse().map_err(serde::de::Error::custom)
}
pub(super) fn allowed(device: DeviceId, feature: &str) -> bool {
    with_state(|s| {
        s.devices.iter().any(|d| d.id == device && matches!(d.link, LinkState::Online { .. }))
            && matches!(
                s.matrices.get(&device).and_then(|m| m.state(feature)),
                Some(FeatureState::Available | FeatureState::Partial { .. })
            )
    })
    .unwrap_or(false)
}
pub(super) fn visible(app: &str) -> bool {
    with_state(|s| s.app_rule(app) != crate::state::AppRule::Hidden).unwrap_or(false)
}
