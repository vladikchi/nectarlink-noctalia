// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared notification-visibility policy for private HTTP reads.
use crate::state::{AppRule, AppState};
use serde_json::{Value, json};
use std::sync::OnceLock;

pub(super) fn visible(s: &AppState, app: &str) -> bool {
    s.app_rule(app) != AppRule::Hidden
}

fn key() -> Option<&'static [u8; 32]> {
    // Not the bearer token: API consumers must not be able to turn these
    // tags into a list of packages that were never visible to them.
    static KEY: OnceLock<Option<[u8; 32]>> = OnceLock::new();
    KEY.get_or_init(|| {
        let mut key = [0; 32];
        crate::win::secure_random(&mut key).then_some(key)
    })
    .as_ref()
}

pub(super) fn ready() -> bool {
    key().is_some()
}

pub(super) fn app_scope(app: &str) -> Option<String> {
    let mut hasher = blake3::Hasher::new_keyed(key()?);
    hasher.update(b"noctalia/notification-app:");
    hasher.update(app.as_bytes());
    Some(hasher.finalize().to_hex().to_string())
}

pub(super) fn policy(s: &AppState) -> Option<Value> {
    let instance = blake3::keyed_hash(key()?, b"noctalia/notification-instance").to_hex().to_string();
    let hidden = s
        .app_rules
        .iter()
        .filter(|(_, rule)| **rule == AppRule::Hidden)
        .filter_map(|(app, _)| app_scope(app))
        .collect::<Vec<_>>();
    Some(json!({
        "instance":instance, "revision":s.notification_visibility_revision.to_string(), "hidden":hidden,
    }))
}

pub(super) fn revision() -> Option<u64> {
    super::with_state(|s| s.notification_visibility_revision)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_visibility_changes_advance_the_policy_and_tags_do_not_contain_app_names() {
        let mut state = AppState::default();
        let app = "org.example.fixture";
        let other = "org.example.other";
        let tag = app_scope(app).unwrap();
        let initial = policy(&state).unwrap();
        state.set_app_rule(app, AppRule::Quiet);
        assert_eq!(initial, policy(&state).unwrap());
        state.set_app_rule(app, AppRule::Hidden);
        let revoked = policy(&state).unwrap();
        assert!(!visible(&state, app));
        assert!(visible(&state, other));
        assert_eq!(revoked["hidden"], json!([tag]));
        assert_eq!(tag.len(), 64);
        assert!(!revoked.to_string().contains(app));
        state.set_app_rule(app, AppRule::Show);
        let restored = policy(&state).unwrap();
        assert_eq!(restored["instance"], initial["instance"]);
        assert_ne!(restored["revision"], initial["revision"]);
        assert_eq!(restored["hidden"], json!([]));
    }
}
