use crate::models::UserId;
use axum::extract::ws::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
pub type Tx = mpsc::UnboundedSender<Message>;

#[derive(Clone)]
pub struct AppState {
    pub connections: Arc<Mutex<HashMap<UserId, Tx>>>,
    pub db: Arc<Mutex<rusqlite::Connection>>,
}
