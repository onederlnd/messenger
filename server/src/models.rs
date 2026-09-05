use serde::{Deserialize, Serialize};

pub type UserId = String;

#[derive(Debug, Serialize)]
pub struct HistoryMessage {
    pub id: i64,
    pub from: UserId,
    pub ciphertext: String,
    pub nonce: String,
    pub self_ciphertext: String,
    pub self_nonce: String,
    pub read: bool,
}

#[derive(Debug, Serialize)]
pub struct FriendInfo {
    pub id: UserId,
    pub username: Option<String>,
    pub avatar: Option<String>,
    pub display_name: Option<String>,
    pub public_key: Option<String>,
    pub last_message: Option<String>,
    pub last_message_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientMessage {
    Offer {
        to: UserId,
        from: UserId,
        sdp: serde_json::Value,
    },
    Answer {
        to: UserId,
        from: UserId,
        sdp: serde_json::Value,
    },
    IceCandidate {
        to: UserId,
        from: UserId,
        candidate: serde_json::Value,
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
    Register {
        username: String,
        password: String,
    },
    Login {
        username: String,
        password: String,
    },
    Logout {
        token: String,
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
    FriendListRequest {
        user_id: UserId,
    },
    HistoryRequest {
        user: UserId,
        with: UserId,
    },
    Typing {
        to: UserId,
        from: UserId,
    },
    Resume {
        token: String,
    },
    SetAvatar {
        user_id: UserId,
        data: String, // base64, stored as a data URL (e.g. "data:image/png;base64,...")
    },
    SetDisplayName {
        user_id: UserId,
        name: String,
    },
    SetPublicKey {
        user_id: UserId,
        key: String,
    },
    ReadReceipt {
        reader: UserId,
        of: UserId,
    },
    SetPushSubscription {
        user_id: UserId,
        subscription: String,
    },
}

#[derive(Debug, Serialize)]
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
    SessionReplaced,
    FriendList {
        friends: Vec<FriendInfo>,
        pending_incoming: Vec<FriendInfo>,
    },
    History {
        with: UserId,
        messages: Vec<HistoryMessage>,
    },
    MessagesRead {
        by: UserId,
        of: UserId,
        message_ids: Vec<i64>,
    },
    MessageAck {
        id: i64,
        client_id: String,
        delivered: bool,
    },
    IncomingMessage {
        id: i64,
        from: UserId,
        ciphertext: String,
        nonce: String,
    },
    Error {
        message: String,
    },
}

impl ClientMessage {
    pub fn target(&self) -> Option<&UserId> {
        match self {
            ClientMessage::Offer { to, .. } => Some(to),
            ClientMessage::Answer { to, .. } => Some(to),
            ClientMessage::IceCandidate { to, .. } => Some(to),
            ClientMessage::Message { .. } => None,
            ClientMessage::Login { .. } => None,
            ClientMessage::Register { .. } => None,
            ClientMessage::FriendRequest { .. } => None,
            ClientMessage::FriendResponse { to, .. } => Some(to),
            ClientMessage::FriendListRequest { .. } => None,
            ClientMessage::HistoryRequest { .. } => None,
            ClientMessage::Typing { to, .. } => Some(to),
            ClientMessage::Logout { .. } => None,
            ClientMessage::Resume { .. } => None,
            ClientMessage::SetAvatar { .. } => None,
            ClientMessage::SetDisplayName { .. } => None,
            ClientMessage::SetPublicKey { .. } => None,
            ClientMessage::ReadReceipt { .. } => None,
            ClientMessage::SetPushSubscription { .. } => None,
        }
    }
    pub fn sender_matches(&self, my_id: &UserId) -> bool {
        match self {
            ClientMessage::Message { from, .. } => from == my_id,
            ClientMessage::Offer { from, .. } => from == my_id,
            ClientMessage::Answer { from, .. } => from == my_id,
            ClientMessage::IceCandidate { from, .. } => from == my_id,
            ClientMessage::Typing { from, .. } => from == my_id,
            ClientMessage::FriendRequest { from, .. } => from == my_id,
            ClientMessage::FriendResponse { to, .. } => to == my_id,
            ClientMessage::FriendListRequest { user_id } => user_id == my_id,
            ClientMessage::HistoryRequest { user, .. } => user == my_id,
            ClientMessage::ReadReceipt { reader, .. } => reader == my_id,
            ClientMessage::SetAvatar { user_id, .. } => user_id == my_id,
            ClientMessage::SetDisplayName { user_id, .. } => user_id == my_id,
            ClientMessage::SetPublicKey { user_id, .. } => user_id == my_id,
            ClientMessage::SetPushSubscription { user_id, .. } => user_id == my_id,
            ClientMessage::Login { .. }
            | ClientMessage::Register { .. }
            | ClientMessage::Resume { .. }
            | ClientMessage::Logout { .. } => true,
        }
    }
}
