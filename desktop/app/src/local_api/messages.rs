// SPDX-License-Identifier: GPL-3.0-or-later
use super::cache::{CachedList, cached_list};
use super::{
    context::{allowed, visible},
    protocol::{Action, Query, sized_window},
};
use nectarlink_core::{Error, Node};
use serde_json::{Value, json};
use std::{sync::OnceLock, time::Duration};

static SMS_LIST: OnceLock<tokio::sync::Mutex<Vec<CachedList<nectarlink_core::SmsThread>>>> = OnceLock::new();
pub(super) async fn read(p: Query, node: Node, limit: usize) -> Result<Value, Error> {
    let device = p.device;
    let sms = allowed(device, "messages.sms");
    let chat = allowed(device, "notifications.mirror");
    if !sms && !chat {
        return Err(Error::Denied);
    }
    if p.thread.is_empty() {
        let mut items = Vec::new();
        // Keep chat conversations usable when SMS permission is denied.
        let mut sms_failed = false;
        if sms && !p.local_only {
            match cached_list(
                &SMS_LIST,
                device,
                p.force,
                Duration::from_secs(20),
                node.sms_threads(device, 100),
            )
            .await
            {
                Ok(threads) => {
                    for t in threads.into_iter().take(100) {
                        items.push(json!({"id":t.id,"title":t.names.iter().filter(|s|!s.is_empty()).cloned().collect::<Vec<_>>().join(", "),
                    "addresses":t.addresses,"subtitle":t.snippet,"date":t.date,"kind":"sms","reply":true}));
                    }
                }
                Err(_) => sms_failed = true,
            }
        }
        if chat {
            for t in node.chat_threads(device).into_iter().filter(|t| visible(&t.app)).take(100) {
                items.push(json!({"id":t.thread_id,"title":t.title,"subtitle":t.snippet,"date":t.updated_ms,"kind":"chat",
                    "reply":t.active && t.reply_action_id.is_some() && allowed(device,"notifications.reply")}));
            }
        }
        items.sort_by_key(|i| std::cmp::Reverse(i["date"].as_i64().unwrap_or(0)));
        let mut result = if p.thread_index {
            // One bounded index for every page of the same conversation list.
            // The caller never turns an incomplete response into a new index.
            json!({"ok":true,"items":items,"more":false})
        } else {
            sized_window(items, p.offset, limit)
        };
        if sms_failed {
            result["smsIncomplete"] = json!(true);
        }
        result["notice"] = json!(if p.local_only {
            ""
        } else if sms_failed {
            "SMS не загружены; доступны сохранённые чаты"
        } else if !sms {
            "SMS недоступны: проверьте разрешение на телефоне"
        } else {
            ""
        });
        Ok(result)
    } else if p.thread.starts_with("chat:") {
        if !chat {
            return Err(Error::Denied);
        }
        let t =
            node.try_chat_thread(device, &p.thread)?.filter(|t| visible(&t.app)).ok_or(Error::NotFound)?;
        let items = node.try_chat_messages(device, &p.thread, 100)?.into_iter().map(|m|
            json!({"id":m.id.to_string(),"title":m.sender.unwrap_or_default(),"subtitle":m.text,"outgoing":m.self_sent,"date":m.time_ms})).collect();
        let mut result = sized_window(items, p.offset, limit);
        result["reply"] =
            json!(t.active && t.reply_action_id.is_some() && allowed(device, "notifications.reply"));
        result["notice"] = json!("Последние 100 сообщений из уведомлений; это не полная история мессенджера");
        Ok(result)
    } else {
        if !sms {
            return Err(Error::Denied);
        }
        let items = node.sms_messages(device, p.thread, p.before, (limit + 1) as u32).await?;
        let more = items.len() > limit;
        let items: Vec<_> = items.into_iter().take(limit).map(|m|json!({"id":m.id,"title":m.address,"subtitle":m.body,"date":m.date,"outgoing":m.outgoing,"status":m.status})).collect();
        Ok(json!({"ok":true,"items":items,"more":more,"reply":true}))
    }
}
pub(super) async fn send(p: Action, node: Node) -> Result<Value, Error> {
    let device = p.device;
    node.sms_send(device, p.recipients, p.text).await?;
    Ok(json!({"ok":true,"accepted":true,"confirmation":"request_only"}))
}
pub(super) async fn reply(p: Action, node: Node) -> Result<Value, Error> {
    let device = p.device;
    let t = node.chat_thread(device, &p.id).filter(|t| visible(&t.app) && t.active).ok_or(Error::Denied)?;
    node.reply_chat_thread(device, &t.thread_id, p.text).await?;
    Ok(json!({"ok":true,"accepted":true,"confirmation":"request_only"}))
}
