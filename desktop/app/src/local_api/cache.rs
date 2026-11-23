// SPDX-License-Identifier: GPL-3.0-or-later
use nectarlink_core::DeviceId;
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};
pub(super) struct CachedList<T> {
    device: DeviceId,
    at: Instant,
    items: Vec<T>,
}
// Cache only bounded read-only lists, never commands or call/session state.
// Permission/online guards run before lookup, and force bypasses old entries.
pub(super) async fn cached_list<T: Clone>(
    slot: &OnceLock<tokio::sync::Mutex<Vec<CachedList<T>>>>,
    device: DeviceId,
    force: bool,
    ttl: Duration,
    fetch: impl std::future::Future<Output = Result<Vec<T>, nectarlink_core::Error>>,
) -> Result<Vec<T>, nectarlink_core::Error> {
    let requested = Instant::now();
    let mut entries = slot.get_or_init(|| tokio::sync::Mutex::new(Vec::new())).lock().await;
    if let Some(e) =
        entries.iter().find(|e| e.device == device && e.at.elapsed() < ttl && (!force || e.at >= requested))
    {
        return Ok(e.items.clone());
    }
    let items = fetch.await?;
    entries.retain(|e| e.device != device);
    if entries.len() >= 2 {
        entries.remove(0);
    }
    entries.push(CachedList { device, at: Instant::now(), items: items.clone() });
    Ok(items)
}
