// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    cache::{CachedList, cached_list},
    context::{allowed, feature_access, visible, with_state},
    privacy,
    protocol::{Action, Query, sized_window},
};
use nectarlink_core::{ChatThreadRecord, DeviceId, Error, Node, SmsThread};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};
use std::{cmp::Reverse, convert::identity, sync::OnceLock, time::Duration};

const SMS_FEATURE: &str = "messages.sms";
const CHAT_FEATURE: &str = "notifications.mirror";
const REPLY_FEATURE: &str = "notifications.reply";
const SOURCE_LIMIT: u32 = 100;
const CHAT_HISTORY_LIMIT: u32 = 100;
const SMS_LIST_TTL: Duration = Duration::from_secs(20);
static SMS_LIST: OnceLock<tokio::sync::Mutex<Vec<CachedList<SmsThread>>>> = OnceLock::new();

/// A row in the combined conversation index, before it crosses the JSON boundary.
#[derive(Serialize)]
struct Conversation {
    id: String,
    title: String,
    subtitle: String,
    date: i64,
    reply: bool,
    #[serde(flatten)]
    source: ConversationSource,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ConversationSource {
    Sms {
        addresses: Vec<String>,
    },
    Chat {
        #[serde(rename = "appScope", serialize_with = "serialize_app_scope")]
        app: String,
    },
}

fn serialize_app_scope<S: Serializer>(app: &str, serializer: S) -> Result<S::Ok, S::Error> {
    privacy::app_scope(app).serialize(serializer)
}

impl Conversation {
    fn sms(mut thread: SmsThread) -> Self {
        thread.names.retain(|name| !name.is_empty());
        Self {
            id: thread.id,
            title: thread.names.join(", "),
            subtitle: thread.snippet,
            date: thread.date,
            reply: true,
            source: ConversationSource::Sms { addresses: thread.addresses },
        }
    }

    fn chat(thread: ChatThreadRecord, can_reply: bool) -> Self {
        Self {
            reply: can_reply && thread.can_reply(),
            id: thread.thread_id,
            title: thread.title,
            subtitle: thread.snippet,
            date: thread.updated_ms,
            source: ConversationSource::Chat { app: thread.app },
        }
    }
}

pub(super) async fn read(query: Query, node: Node, limit: usize) -> Result<Value, Error> {
    if query.thread.is_empty() {
        read_conversations(&query, &node, limit).await
    } else if query.thread.starts_with("chat:") {
        read_chat_history(&query, &node, limit)
    } else {
        read_sms_history(query, &node, limit).await
    }
}

/// Apply current chat visibility after phone I/O, with no app-state lock over storage reads.
fn append_local_chats(node: &Node, device: DeviceId, rows: &mut Vec<Conversation>) -> Result<(), Error> {
    let access = with_state(|state| feature_access(state, device, CHAT_FEATURE)).flatten();
    match access {
        Some(false) => return Ok(()),
        None => return Err(Error::Offline),
        Some(true) => {}
    }
    let threads = node.try_chat_threads(device)?;
    with_state(|state| match feature_access(state, device, CHAT_FEATURE) {
        Some(true) => {
            let can_reply = feature_access(state, device, REPLY_FEATURE) == Some(true);
            rows.extend(
                threads
                    .into_iter()
                    .filter(|thread| privacy::visible(state, &thread.app))
                    .take(SOURCE_LIMIT as usize)
                    .map(|thread| Conversation::chat(thread, can_reply)),
            );
            Ok(())
        }
        Some(false) => Ok(()),
        None => Err(Error::Offline),
    })
    .unwrap_or(Err(Error::Offline))
}

/// Read the two independent sources; mark partial results instead of clearing a good index.
async fn read_conversations(query: &Query, node: &Node, limit: usize) -> Result<Value, Error> {
    let device = query.device;
    let (can_read_sms, can_read_chats) = with_state(|state| {
        (
            feature_access(state, device, SMS_FEATURE) == Some(true),
            feature_access(state, device, CHAT_FEATURE) == Some(true),
        )
    })
    .unwrap_or_default();
    if !can_read_sms && !can_read_chats {
        return Err(Error::Denied);
    }

    let sms = if can_read_sms && !query.local_only {
        cached_list(&SMS_LIST, device, query.force, SMS_LIST_TTL, node.sms_threads(device, SOURCE_LIMIT))
            .await
    } else {
        Ok(Vec::new())
    };
    let sms_incomplete = sms.is_err();
    let mut rows = sms
        .unwrap_or_default()
        .into_iter()
        .take(SOURCE_LIMIT as usize)
        .map(Conversation::sms)
        .collect::<Vec<_>>();
    let mut local_chats_incomplete = false;
    if can_read_chats && let Err(error) = append_local_chats(node, device, &mut rows) {
        if query.local_only {
            return Err(error);
        }
        local_chats_incomplete = true;
    }
    let mut result = conversation_window(rows, query.thread_index, query.offset, limit);
    if sms_incomplete {
        result["smsIncomplete"] = json!(true);
    }
    if local_chats_incomplete {
        result["localChatsIncomplete"] = json!(true);
    }
    result["notice"] = json!(if query.local_only {
        ""
    } else if sms_incomplete {
        "SMS не загружены; доступны сохранённые чаты"
    } else if !can_read_sms {
        "SMS недоступны: проверьте разрешение на телефоне"
    } else {
        ""
    });
    Ok(result)
}

fn conversation_window(mut rows: Vec<Conversation>, index: bool, offset: usize, limit: usize) -> Value {
    // Keep the order from each source for rows sharing the same timestamp.
    rows.sort_by_key(|row| Reverse(row.date));
    let (offset, limit) = if index { (0, rows.len()) } else { (offset, limit) };
    sized_window(rows.into_iter(), offset, limit, identity)
}

fn read_chat_history(query: &Query, node: &Node, limit: usize) -> Result<Value, Error> {
    let device = query.device;
    if !allowed(device, CHAT_FEATURE) {
        return Err(Error::Denied);
    }
    let thread = node
        .try_chat_thread(device, &query.thread)?
        .filter(|thread| visible(&thread.app))
        .ok_or(Error::NotFound)?;
    let messages = node.try_chat_messages(device, &query.thread, CHAT_HISTORY_LIMIT)?;
    let mut result = sized_window(messages.into_iter(), query.offset, limit, |message| {
        json!({
            "id":message.id.to_string(), "title":message.sender.unwrap_or_default(),
            "subtitle":message.text, "outgoing":message.self_sent, "date":message.time_ms,
        })
    });
    result["reply"] = json!(thread.can_reply() && allowed(device, REPLY_FEATURE));
    result["appScope"] = json!(privacy::app_scope(&thread.app));
    result["notice"] = json!("Последние 100 сообщений из уведомлений; это не полная история мессенджера");
    Ok(result)
}

async fn read_sms_history(query: Query, node: &Node, limit: usize) -> Result<Value, Error> {
    if !allowed(query.device, SMS_FEATURE) {
        return Err(Error::Denied);
    }
    let messages = node.sms_messages(query.device, query.thread, query.before, (limit + 1) as u32).await?;
    let mut result = sized_window(messages.into_iter(), 0, limit, |message| {
        json!({
            "id":message.id, "title":message.address, "subtitle":message.body,
            "date":message.date, "outgoing":message.outgoing, "status":message.status,
        })
    });
    result["reply"] = json!(true);
    Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn conversations() -> Vec<Conversation> {
        let sms = |id: &str, date| {
            Conversation::sms(SmsThread {
                id: id.into(),
                names: vec!["One".into(), String::new(), "Two".into()],
                addresses: vec!["100".into(), "101".into()],
                snippet: String::new(),
                date,
                unread: 0,
                photo: None,
            })
        };
        let chat = Conversation::chat(
            ChatThreadRecord {
                peer: DeviceId([8; 32]),
                thread_id: "chat:fixture".into(),
                app: "org.example.fixture".into(),
                app_name: String::new(),
                notification_key: String::new(),
                reply_action_id: Some("reply".into()),
                title: "Fixture".into(),
                is_group: false,
                snippet: String::new(),
                last_sender: None,
                updated_ms: 4_000,
                unread: 0,
                active: true,
                avatar: None,
                app_icon: None,
            },
            false, // The phone offers a reply, but the local feature is not allowed.
        );
        vec![sms("first", 4_000), sms("older", 3_000), chat]
    }

    #[test]
    fn conversation_index_preserves_the_legacy_wire_shape_and_order() {
        let index = conversation_window(conversations(), true, 12, 1);
        assert_eq!(index["more"], false);
        let items = index["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0],
            json!({
                "id":"first", "title":"One, Two", "subtitle":"", "date":4000, "reply":true,
                "kind":"sms", "addresses":["100","101"],
            })
        );
        assert_eq!(
            items[1],
            json!({
                "id":"chat:fixture", "title":"Fixture", "subtitle":"", "date":4000, "reply":false,
                "kind":"chat", "appScope":privacy::app_scope("org.example.fixture"),
            })
        );
        assert_eq!(items[2]["id"], "older");

        let page = conversation_window(conversations(), false, 1, 1);
        assert_eq!(page["items"], json!([items[1]]));
        assert_eq!(page["more"], true);
    }
}
