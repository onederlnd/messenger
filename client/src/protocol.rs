// client/src/protocol.rs

use serde::{Deserialize, Serialize};

pub type UserId = String;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientMessage {
    Login {
        username: String,
        password: String,
    },
    Register {
        username: String,
        password: String,
    },
    Resume {
        token: String,
    },
    FriendListRequest {
        user_id: UserId,
    },
    FriendRequest {
        to_username: String,
        from: UserId,
    },
    FriendResponse {
        from: UserId,
        to: UserId,
        accept: bool,
    },
    Message {
        to: UserId,
        from: UserId,
        client_id: String,
        ciphertext: String,
        nonce: String,
        self_ciphertext: String,
        self_nonce: String,
    },
    HistoryRequest {
        user: UserId,
        with: UserId,
    },
    ReadReceipt {
        reader: UserId,
        of: UserId,
    },
    SetPublicKey {
        user_id: String,
        key: String,
    },
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServerMessage {
    AuthSuccess {
        token: String,
        id: UserId,
        username: String,
    },
    AuthError {
        message: String,
    },
    FriendList {
        friends: Vec<FriendInfo>,
        pending_incoming: Vec<FriendInfo>,
    },
    IncomingMessage {
        id: i64,
        from: UserId,
        ciphertext: String,
        nonce: String,
    },
    MessageAck {
        id: i64,
        client_id: String,
        delivered: bool,
    },
    History {
        with: UserId,
        messages: Vec<HistoryMessage>,
    },
    SessionReplaced,
    MessagesRead {
        by: UserId,
        of: UserId,
        message_ids: Vec<i64>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriendInfo {
    pub id: UserId,
    pub username: Option<String>,
    pub avatar: Option<String>,
    pub display_name: Option<String>,
    pub public_key: Option<String>,
    pub last_message: Option<String>,
    pub last_message_time: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriendRequest {
    to_username: String,
    from: UserId,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HistoryMessage {
    pub id: i64,
    pub from: UserId,
    pub ciphertext: String,
    pub nonce: String,
    pub self_ciphertext: String,
    pub self_nonce: String,
    pub read: bool,
}
