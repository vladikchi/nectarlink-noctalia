// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    cache::{CachedList, cached_list, lookup},
    context::allowed,
    protocol::{Action, Query},
};
use nectarlink_core::{DeviceId, Error, Node, PhotoAlbum};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::OnceLock, time::Duration};

static ALBUMS: OnceLock<tokio::sync::Mutex<Vec<CachedList<PhotoAlbum>>>> = OnceLock::new();
const ALBUM_TTL: Duration = Duration::from_secs(300);

fn cached_paths(device: DeviceId, ids: &[String]) -> HashMap<String, String> {
    ids.iter()
        .filter_map(|id| {
            let path = crate::photos::thumb_path(device, id);
            path.is_file().then(|| (id.clone(), path.to_string_lossy().into_owned()))
        })
        .collect()
}

async fn details(
    device: DeviceId,
    node: &Node,
    ids: &[String],
    force: bool,
) -> (HashMap<String, String>, Option<Vec<PhotoAlbum>>, bool) {
    let mut paths = cached_paths(device, ids);
    let missing: Vec<_> = ids.iter().filter(|id| !paths.contains_key(*id)).cloned().collect();
    let (thumbs, albums) = tokio::join!(
        async {
            if missing.is_empty() { Ok(Vec::new()) } else { node.photo_thumbs(device, missing.clone()).await }
        },
        cached_list(&ALBUMS, device, force, ALBUM_TTL, node.photo_albums(device))
    );
    let incomplete = thumbs.is_err() || albums.is_err();
    if let Ok(thumbs) = thumbs
        && !thumbs.is_empty()
    {
        let written = tokio::task::spawn_blocking(move || {
            let mut written = HashMap::new();
            for thumb in thumbs {
                if missing.contains(&thumb.id)
                    && thumb.data.len() <= 128 * 1024
                    && let Some(path) = crate::photos::write_thumb(device, &thumb.id, &thumb.data)
                {
                    written.insert(thumb.id, path.to_string_lossy().into_owned());
                }
            }
            // Apply the existing gallery's age, file and byte limits even
            // when all thumbnail requests come from the Noctalia panel.
            if !written.is_empty() {
                crate::photos::prune_cache();
            }
            written
        })
        .await;
        if let Ok(written) = written {
            paths.extend(written);
        }
    }
    (paths, albums.ok(), incomplete)
}

pub(super) async fn read(p: Query, node: Node, _limit: usize) -> Result<Value, Error> {
    let device = p.device;
    if !allowed(device, "files.recent_photos") {
        return Err(Error::Denied);
    }
    if let Some(ids) = p.photo_ids {
        let (paths, albums, incomplete) = details(device, &node, &ids, p.force).await;
        let mut result = json!({
            "ok":true,
            "items": paths.into_iter().map(|(id,path)| json!({"id":id,"thumbnailPath":path})).collect::<Vec<_>>(),
            "photoDetailsIncomplete": incomplete,
        });
        if let Some(albums) = albums {
            result["albums"] = json!(albums);
        }
        return Ok(result);
    }
    let before = p.before.map(|date| (date, p.before_id));
    let rows = node.photo_list(device, p.album, before, 9).await?;
    let more = rows.len() > 8;
    let rows: Vec<_> = rows.into_iter().take(8).collect();
    let ids: Vec<_> = rows.iter().map(|r| r.id.clone()).collect();
    let (paths, albums) = if p.photo_rows_only {
        (cached_paths(device, &ids), lookup(&ALBUMS, device, ALBUM_TTL, |albums| Some(albums.to_vec())))
    } else {
        let (paths, albums, _) = details(device, &node, &ids, p.force).await;
        (paths, Some(albums.unwrap_or_default()))
    };
    let items: Vec<_> = rows.into_iter().map(|r| {
        json!({"id":r.id,"title":r.name,"date":r.date,"size":r.size,"thumbnailPath":paths.get(&r.id),"duration":r.duration})
    }).collect();
    let mut result = json!({"ok":true,"items":items,"more":more});
    if let Some(albums) = albums {
        result["albums"] = json!(albums);
    }
    Ok(result)
}
pub(super) async fn fetch(p: Action, node: Node) -> Result<Value, Error> {
    let device = p.device;
    let transfer = node.fetch_photo(device, p.id).await?;
    return Ok(json!({"ok":true,"accepted":true,"transfer":transfer}));
}
