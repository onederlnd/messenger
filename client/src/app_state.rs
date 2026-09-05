// client/src/app_state.rs

// TODO: currently unused — wire this in to replace the ad-hoc state below.
//   - FriendListState.friend_keys replaces the standalone `friend_keys: FriendKeys`
//     in lib.rs/websocket.rs (same HashMap<UserId, String>, just owned here).
//   - MessageState replaces pushing straight into the Slint VecModel<MessageData>:
//     websocket.rs (History, IncomingMessage) and lib.rs (on_send_message) should
//     insert/append ChatMessage into MessageState.0, then a separate sync step
//     converts the active conversation's Vec<ChatMessage> into MessageData for
//     the UI model. This is what lets MessageAck/MessagesRead update
//     ChatMessage.delivered/.read in place instead of needing new Slint fields.
//   - AppScreen replaces the implicit state machine currently spread across
//     app.get_logged_in() / active_chat_friend_id checks; on_open_chat/
//     on_close_chat/on_login_clicked etc. should set AppScreen instead.
//   - LoginError replaces the raw String in app.set_login_error.
// Needs an Arc<Mutex<...>> (or similar) wrapper per struct, owned in run_app()
// and cloned into closures the same way friend_keys/secret_key are now.

use crate::protocol::FriendInfo;
type UserId = String;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum AppScreen {
    Login,
    LoggingIn,
    FriendList,
    Chat,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoginError(pub Option<String>);

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FriendListState {
    pub friends: Vec<FriendInfo>,
    pub pending_incoming: Vec<FriendInfo>,
    pub friend_keys: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub id: Option<i64>,
    pub client_id: String,
    pub from: UserId,
    pub text: String,
    pub delivered: bool,
    pub read: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MessageState(pub HashMap<UserId, Vec<ChatMessage>>);
