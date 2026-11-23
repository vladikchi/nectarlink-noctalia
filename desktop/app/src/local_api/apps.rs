// SPDX-License-Identifier: GPL-3.0-or-later
use super::cache::{CachedList, cached_list};
use super::{
    context::allowed,
    protocol::{Action, Query},
};
use nectarlink_core::{Error, Node};
use serde_json::{Value, json};
use std::{sync::OnceLock, time::Duration};

use super::previews::thumbnails;
static APP_LIST: OnceLock<tokio::sync::Mutex<Vec<CachedList<nectarlink_core::PhoneApp>>>> = OnceLock::new();
pub(super) async fn read(p: Query, node: Node, _limit: usize) -> Result<Value, Error> {
    let device = p.device;
    if !allowed(device, "mirroring.app_windows") {
        return Err(Error::Denied);
    }
    let search = p.search.to_lowercase();
    let apps: Vec<_> =
        cached_list(&APP_LIST, device, p.force, Duration::from_secs(120), node.mirror_apps(device))
            .await?
            .into_iter()
            .filter(|r| r.label.to_lowercase().contains(&search) || r.pkg.to_lowercase().contains(&search))
            .collect();
    let app_limit = p.limit.unwrap_or(12).min(96);
    let more = apps.len() > p.offset.saturating_add(app_limit);
    let apps: Vec<_> = apps.into_iter().skip(p.offset).take(app_limit).collect();
    let icons: Vec<_> = apps
        .iter()
        .filter_map(|a| {
            a.icon.as_ref().map(|data| nectarlink_core::PhotoThumb { id: a.pkg.clone(), data: data.clone() })
        })
        .collect();
    let paths = thumbnails(&icons, "png");
    let items: Vec<_> = apps
        .into_iter()
        .map(|r| json!({"id":r.pkg,"title":r.label,"subtitle":r.pkg,"iconPath":paths.get(&r.pkg)}))
        .collect();
    Ok(json!({"ok":true,"items":items,"more":more}))
}
pub(super) async fn launch(p: Action, node: Node) -> Result<Value, Error> {
    let device = p.device;
    let app = node.mirror_apps(device).await?.into_iter().find(|a| a.pkg == p.id).ok_or(Error::NotFound)?;
    #[cfg(not(test))]
    crate::mirror::start_app(device, app.pkg, app.label);
    #[cfg(test)]
    let _ = app;
    Ok(json!({"ok":true,"accepted":true,"confirmation":"request_only"}))
}
