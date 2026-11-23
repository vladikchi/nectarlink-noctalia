// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    context::allowed,
    protocol::{Action, Query},
};
use nectarlink_core::{Error, Node};
use serde_json::{Value, json};

use super::previews::thumbnails;
pub(super) async fn read(p: Query, node: Node, _limit: usize) -> Result<Value, Error> {
    let device = p.device;
    if !allowed(device, "files.recent_photos") {
        return Err(Error::Denied);
    }
    let before = p.before.map(|date| (date, p.before_id));
    let rows = node.photo_list(device, p.album, before, 9).await?;
    let more = rows.len() > 8;
    let rows: Vec<_> = rows.into_iter().take(8).collect();
    // Independent read-only requests: albums must not wait for thumbnails.
    let (thumbs, albums) = tokio::join!(
        node.photo_thumbs(device, rows.iter().map(|r| r.id.clone()).collect()),
        node.photo_albums(device)
    );
    let thumbs = thumbs.unwrap_or_default();
    let albums = albums.unwrap_or_default();
    let paths = thumbnails(&thumbs, "jpg");
    let items: Vec<_> = rows.into_iter().map(|r| {
        json!({"id":r.id,"title":r.name,"date":r.date,"size":r.size,"thumbnailPath":paths.get(&r.id),"duration":r.duration})
    }).collect();
    Ok(json!({"ok":true,"items":items,"more":more,"albums":albums}))
}
pub(super) async fn fetch(p: Action, node: Node) -> Result<Value, Error> {
    let device = p.device;
    let transfer = node.fetch_photo(device, p.id).await?;
    return Ok(json!({"ok":true,"accepted":true,"transfer":transfer}));
}
