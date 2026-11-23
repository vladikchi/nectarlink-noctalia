// SPDX-License-Identifier: GPL-3.0-or-later
use super::protocol::Action;
use nectarlink_core::Error;
use serde_json::{Value, json};
pub(super) fn deck_view(items: Vec<Value>) -> Value {
    // Core DeckConfig is bounded to 8 pages of 24 tiles. Public DTOs contain
    // display fields only; never local command/path parameters.
    json!({"ok":true,"items":items.into_iter().take(192).collect::<Vec<_>>(),"more":false,"complete":true})
}
pub(super) fn read() -> Result<Value, Error> {
    #[cfg(not(test))]
    {
        let cfg = crate::deck::config();
        let items = cfg
            .pages
            .iter()
            .flat_map(|page| {
                page.tiles
                    .iter()
                    .map(|t| json!({"id":t.id,"title":t.label,"subtitle":page.name,"kind":t.action.kind()}))
            })
            .collect();
        return Ok(deck_view(items));
    }
    #[cfg(test)]
    return Ok(deck_view(vec![]));
}
pub(super) async fn act(p: Action) -> Result<Value, Error> {
    #[cfg(not(test))]
    crate::deck::execute_tile(&p.id).map_err(|_| Error::Denied)?;
    #[cfg(test)]
    {
        let _ = p;
        Err(Error::Offline)
    }
    #[cfg(not(test))]
    {
        Ok(json!({"ok":true,"accepted":true,"confirmation":"request_only"}))
    }
}
