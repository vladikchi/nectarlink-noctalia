// SPDX-License-Identifier: GPL-3.0-or-later
//! Existing v1 page request DTOs and bounded list responses, not UI/business logic.
use super::context::display_device_id;
use nectarlink_core::DeviceId;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Query {
    #[serde(deserialize_with = "display_device_id")]
    pub(super) device: DeviceId,
    pub(super) page: String,
    #[serde(default)]
    pub(super) thread: String,
    #[serde(default)]
    pub(super) album: Option<String>,
    #[serde(default)]
    pub(super) before: Option<i64>,
    #[serde(default)]
    pub(super) before_id: String,
    #[serde(default)]
    pub(super) offset: usize,
    #[serde(default)]
    pub(super) search: String,
    #[serde(default)]
    pub(super) limit: Option<usize>,
    #[serde(default)]
    pub(super) force: bool,
    #[serde(default)]
    pub(super) local_only: bool,
    #[serde(default)]
    pub(super) thread_index: bool,
    #[serde(default)]
    pub(super) photo_rows_only: bool,
    pub(super) photo_ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Action {
    #[serde(deserialize_with = "display_device_id")]
    pub(super) device: DeviceId,
    pub(super) action: String,
    pub(super) request_id: String,
    pub(super) confirm: bool,
    #[serde(default)]
    pub(super) id: String,
    #[serde(default)]
    pub(super) text: String,
    #[serde(default)]
    pub(super) recipients: Vec<String>,
}

pub(super) fn sized_window(items: Vec<Value>, offset: usize, limit: usize) -> Value {
    let more = items.len() > offset.saturating_add(limit);
    json!({"ok":true,"items":items.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"more":more})
}
