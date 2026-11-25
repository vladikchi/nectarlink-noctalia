// SPDX-License-Identifier: GPL-3.0-or-later
//! Existing v1 page request DTOs and bounded list responses, not UI/business logic.
use super::context::display_device_id;
use nectarlink_core::DeviceId;
use serde::{Deserialize, Serialize};
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

/// Apply the JSON projection after selecting the rows to return.
pub(super) fn sized_window<T, R: Serialize>(
    items: impl ExactSizeIterator<Item = T>,
    offset: usize,
    limit: usize,
    project: impl FnMut(T) -> R,
) -> Value {
    let more = items.len() > offset.saturating_add(limit);
    let items = items.skip(offset).take(limit).map(project).collect::<Vec<_>>();
    json!({"ok":true,"items":items,"more":more})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_window_projects_only_the_rows_it_returns() {
        let calls = Cell::new(0);
        let page = sized_window(0..100, 31, 7, |id| {
            calls.set(calls.get() + 1);
            json!({"id": id})
        });
        assert_eq!(calls.get(), 7);
        assert_eq!(page["items"][0]["id"], 31);
        assert_eq!(page["items"][6]["id"], 37);
        assert_eq!(page["more"], true);
    }
}
