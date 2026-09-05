use crate::models::{FriendInfo, ServerMessage, UserId};
use crate::state::AppState;
use axum::extract::ws::Message;

pub async fn send_friend_list(state: &AppState, user_id: &UserId) {
    let db = state.db.lock().await;

    let friends: Vec<FriendInfo> = {
        let mut stmt = db
            .prepare(
                "SELECT u.uuid, u.id, u.avatar, u.display_name, u.public_key,
                    (SELECT payload FROM messages m
                        WHERE (m.to_id = u.uuid AND m.from_id = ?1) OR (m.to_id = ?1 AND m.from_id = u.uuid)
                        ORDER BY m.id DESC LIMIT 1) as last_payload,
                    (SELECT created_at from messages m
                        WHERE (m.to_id = u.uuid AND m.from_id = ?1) OR (m.to_id = ?1 AND m.from_id = u.uuid)
                        ORDER BY m.id DESC LIMIT 1) as last_created_at
                FROM friends f
                JOIN users u ON u.uuid = CASE WHEN f.requester_id = ?1 THEN f.addressee_id ELSE f.requester_id END
                WHERE (f.requester_id = ?1 OR f.addressee_id = ?1) AND f.status = 'accepted'",
            )
            .unwrap();
        stmt.query_map([user_id], |row| {
            let last_payload: Option<String> = row.get(5)?;
            let last_created_at: Option<String> = row.get(6)?;
            let last_message = last_payload.as_ref().and_then(|p| {
                serde_json::from_str::<serde_json::Value>(p)
                    .ok()
                    .and_then(|v| v.get("ciphertext")?.as_str().map(String::from))
            });
            Ok(FriendInfo {
                id: row.get(0)?,
                username: row.get(1)?,
                avatar: row.get(2)?,
                display_name: row.get(3)?,
                public_key: row.get(4)?,
                last_message,
                last_message_time: last_created_at,
            })
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
    };

    let pending_incoming: Vec<FriendInfo> = {
        let mut stmt2 = db
            .prepare(
                "SELECT u.uuid, u.id, u.avatar, u.display_name, u.public_key FROM friends f
                JOIN users u ON u.uuid = f.requester_id
                WHERE f.addressee_id = ?1 AND f.status = 'pending'",
            )
            .unwrap();
        stmt2
            .query_map([user_id], |row| {
                Ok(FriendInfo {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    avatar: row.get(2)?,
                    display_name: row.get(3)?,
                    public_key: row.get(4)?,
                    last_message: None,
                    last_message_time: None,
                })
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect()
    };

    drop(db);

    let response = ServerMessage::FriendList {
        friends,
        pending_incoming,
    };
    let msg = serde_json::to_string(&response).unwrap();
    if let Some(target_tx) = state.connections.lock().await.get(user_id) {
        let _ = target_tx.send(Message::Text(msg.into()));
    }
}
