// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    context::allowed,
    protocol::{Action, Query},
};
use nectarlink_core::{Error, Node};
use serde_json::{Value, json};

#[cfg(not(test))]
use nectarlink_core::CallCommand;
pub(super) async fn read(p: Query, node: Node, limit: usize) -> Result<Value, Error> {
    let device = p.device;
    let mut result = json!({"ok":true,"items":[],"more":false,"dial":allowed(device,"calls.dial")});
    #[cfg(not(test))]
    {
        result["active"] = json!(
            crate::calls::active(device)
                .map(|c| json!({"id":c.id,"state":c.state,"name":c.name,"number":c.number}))
        );
        result["control"] = json!(crate::calls::can_control(device));
    }
    if allowed(device, "calls.log") {
        let rows = node.call_log(device, p.before, (limit + 1) as u32).await?;
        result["more"] = json!(rows.len() > limit);
        result["items"] = json!(rows.into_iter().take(limit).map(|r|json!({"id":r.id,"title":r.name.unwrap_or_else(||r.number.clone()),"number":r.number,"subtitle":r.direction,"date":r.date,"duration":r.duration})).collect::<Vec<_>>());
    } else {
        result["notice"] = json!("История звонков недоступна с текущими разрешениями");
    }
    Ok(result)
}
pub(super) async fn dial(p: Action, node: Node) -> Result<Value, Error> {
    let device = p.device;
    node.call_dial(device, p.text, None).await?;
    Ok(json!({"ok":true,"accepted":true,"confirmation":"request_only"}))
}
pub(super) async fn control(p: Action, node: Node) -> Result<Value, Error> {
    #[cfg(not(test))]
    {
        let device = p.device;
        let active = crate::calls::active(device).filter(|c| c.id == p.id).ok_or(Error::NotFound)?;
        let command = match p.action.as_str() {
            "answer" => CallCommand::Answer,
            "decline" => CallCommand::Decline,
            "silence" => CallCommand::Silence,
            _ => return Err(Error::Denied),
        };
        node.call_command(device, active.id, command).await?;
    }
    #[cfg(test)]
    {
        let _ = (p, node);
        Err(Error::Offline)
    }
    #[cfg(not(test))]
    {
        Ok(json!({"ok":true,"accepted":true,"confirmation":"request_only"}))
    }
}
