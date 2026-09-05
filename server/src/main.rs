mod auth;
mod handlers;
mod models;
mod push;
mod rate_limit;
mod state;

use auth::{generate_session_token, hash_password, verify_password};
use axum::extract::ConnectInfo;
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use axum_server::tls_rustls::RustlsConfig;
use futures::{SinkExt, StreamExt};
use handlers::send_friend_list;
use models::{ClientMessage, HistoryMessage, ServerMessage, UserId};
use rustls_pemfile::{certs, pkcs8_private_keys};
use state::AppState;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

const MIN_USERNAME_LEN: usize = 3;
const MAX_USERNAME_LEN: usize = 32;
const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 256;
const MAX_DISPLAY_NAME_LEN: usize = 64;

// Incoming messages from client -- one enum covers every message shape.
// serde picks the right variant based on the "type" field.

fn build_rustls_config(cert_path: &str, key_path: &str) -> rustls::ServerConfig {
    let cert_file = &mut BufReader::new(File::open(cert_path).unwrap());
    let key_file = &mut BufReader::new(File::open(key_path).unwrap());

    let cert_chain: Vec<_> = certs(cert_file).collect::<Result<_, _>>().unwrap();
    let key = pkcs8_private_keys(key_file).next().unwrap().unwrap();

    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, rustls::pki_types::PrivateKeyDer::Pkcs8(key))
        .unwrap();

    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let tls_cert = std::env::var("TLS_CERT_PATH").unwrap_or_else(|_| "localhost.pem".to_string());
    let tls_key = std::env::var("TLS_KEY_PATH").unwrap_or_else(|_| "localhost-key.pem".to_string());
    let bind_ip: [u8; 4] = std::env::var("BIND_IP")
        .ok()
        .and_then(|s| {
            let parts: Vec<u8> = s.split('.').filter_map(|p| p.parse().ok()).collect();
            if parts.len() == 4 {
                Some([parts[0], parts[1], parts[2], parts[3]])
            } else {
                None
            }
        })
        .unwrap_or([0, 0, 0, 0]);
    let bind_port: u16 = std::env::var("BIND_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);

    let conn = rusqlite::Connection::open("messages.db").unwrap();

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            to_id TEXT NOT NULL,
            from_id TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            delivered INTEGER NOT NULL DEFAULT 0,
            read INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            uuid TEXT UNIQUE,
            display_name TEXT,
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            avatar TEXT,
            public_key TEXT,
            push_subscription TEXT
        );

        CREATE TABLE IF NOT EXISTS friends (
            requester_id TEXT NOT NULL,
            addressee_id TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (requester_id, addressee_id)
        );

        CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            expires_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS login_attempts (
            key TEXT PRIMARY KEY,
            failure_count INTEGER NOT NULL DEFAULT 0,
            locked_until INTEGER
        );
        ",
    )
    .unwrap();

    let state = AppState {
        connections: Arc::new(Mutex::new(HashMap::new())),
        db: Arc::new(Mutex::new(conn)),
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    let rustls_config = build_rustls_config(&tls_cert, &tls_key);
    let tls_config = RustlsConfig::from_config(Arc::new(rustls_config));

    let addr = std::net::SocketAddr::from((bind_ip, bind_port));

    axum_server::bind_rustls(addr, tls_config)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, addr))
}

pub async fn handle_socket(
    socket: axum::extract::ws::WebSocket,
    state: AppState,
    addr: std::net::SocketAddr,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let my_id = match ws_receiver.next().await {
        Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Login { username, password }) => {
                let ip_key = addr.ip().to_string();
                let db = state.db.lock().await;

                let locked =
                    rate_limit::is_locked(&db, &username) || rate_limit::is_locked(&db, &ip_key);

                if locked {
                    drop(db);
                    let err = ServerMessage::AuthError {
                        message: "Too many attempts, try again later".to_string(),
                    };
                    let _ = ws_sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                    return;
                }

                let _ = db.execute(
                    "DELETE FROM sessions WHERE expires_at <= CURRENT_TIMESTAMP",
                    [],
                );

                let row: Option<(String, String)> = db
                    .query_row(
                        "SELECT password_hash, uuid FROM users WHERE id = ?1",
                        [&username],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .ok();

                match row {
                    Some((hash, uuid)) if verify_password(&password, &hash) => {
                        rate_limit::record_success(&db, &username);
                        let token = generate_session_token();
                        let _ = db.execute(
                            "INSERT INTO sessions (token, user_id, expires_at) VALUES (?1,?2, datetime('now', '+30 days'))",
                            rusqlite::params![token, uuid],
                        );
                        drop(db);
                        let msg = serde_json::to_string(&ServerMessage::AuthSuccess {
                            token,
                            id: uuid.clone(),
                            username: username.clone(),
                        })
                        .unwrap();
                        let _ = ws_sender.send(Message::Text(msg.into())).await;
                        uuid
                    }
                    _ => {
                        rate_limit::record_failure(&db, &username);
                        rate_limit::record_failure(&db, &ip_key);

                        drop(db);

                        let err = ServerMessage::AuthError {
                            message: "Invalid username or password".to_string(),
                        };
                        let msg = serde_json::to_string(&err).unwrap();
                        let _ = ws_sender.send(Message::Text(msg.into())).await;
                        return;
                    }
                }
            }
            Ok(ClientMessage::Register { username, password }) => {
                if username.trim().len() < MIN_USERNAME_LEN
                    || username.len() > MAX_USERNAME_LEN
                    || !username
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                {
                    let err = ServerMessage::AuthError {
                        message: "Username must be 3-32 characters, letters/numbers/_/- only"
                            .to_string(),
                    };
                    let _ = ws_sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                    return;
                }

                if password.len() < MIN_PASSWORD_LEN || password.len() > MAX_PASSWORD_LEN {
                    let err = ServerMessage::AuthError {
                        message: "Password must be 8-256 characters".to_string(),
                    };
                    let _ = ws_sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                    return;
                }

                let ip_key = addr.ip().to_string();
                let hash = hash_password(&password);
                let new_uuid = uuid::Uuid::new_v4().to_string();
                let db = state.db.lock().await;

                if rate_limit::is_locked(&db, &ip_key) {
                    drop(db);
                    let err = ServerMessage::AuthError {
                        message: "Too many attempts, try again later".to_string(),
                    };
                    let _ = ws_sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                    return;
                }

                let _ = db.execute(
                    "DELETE FROM sessions WHERE expires_at <= CURRENT_TIMESTAMP",
                    [],
                );

                let result = db.execute(
                    "INSERT INTO users (id, uuid, password_hash) VALUES (?1, ?2, ?3)",
                    rusqlite::params![username, new_uuid, hash],
                );
                match result {
                    Ok(_) => {
                        let token = generate_session_token();
                        let _ = db.execute(
                            "INSERT INTO sessions (token, user_id, expires_at) VALUES (?1,?2, datetime('now', '+30 days'))",
                            rusqlite::params![token, new_uuid],
                        );
                        drop(db);
                        let msg = serde_json::to_string(&ServerMessage::AuthSuccess {
                            token,
                            id: new_uuid.clone(),
                            username: username.clone(),
                        })
                        .unwrap();
                        let _ = ws_sender.send(Message::Text(msg.into())).await;
                        new_uuid
                    }
                    Err(_) => {
                        rate_limit::record_failure(&db, &ip_key);
                        drop(db);
                        let err = ServerMessage::AuthError {
                            message: "Username taken".to_string(),
                        };
                        let msg = serde_json::to_string(&err).unwrap();
                        let _ = ws_sender.send(Message::Text(msg.into())).await;
                        return;
                    }
                }
            }
            Ok(ClientMessage::Resume { token }) => {
                let db = state.db.lock().await;
                let row: Option<(String, String)> = db
                    .query_row(
                        "SELECT s.user_id, u.id FROM sessions s JOIN users u ON u.uuid = s.user_id 
                        WHERE s.token = ?1 AND s.expires_at > CURRENT_TIMESTAMP",
                        [&token],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .ok();

                drop(db);

                match row {
                    Some((uuid, username)) => {
                        let msg = serde_json::to_string(&ServerMessage::AuthSuccess {
                            token,
                            id: uuid.clone(),
                            username,
                        })
                        .unwrap();
                        let _ = ws_sender.send(Message::Text(msg.into())).await;
                        uuid
                    }
                    None => {
                        let err = ServerMessage::AuthError {
                            message: "Session expired, please log in".to_string(),
                        };
                        let msg = serde_json::to_string(&err).unwrap();
                        let _ = ws_sender.send(Message::Text(msg.into())).await;
                        return;
                    }
                }
            }
            _ => {
                let err = ServerMessage::AuthError {
                    message: "Expected login or register".to_string(),
                };
                let msg = serde_json::to_string(&err).unwrap();
                let _ = ws_sender.send(Message::Text(msg.into())).await;
                return;
            }
        },
        _ => return,
    };
    {
        let mut conns = state.connections.lock().await;
        if let Some(old_tx) = conns.get(&my_id) {
            let msg = serde_json::to_string(&ServerMessage::SessionReplaced).unwrap();
            let _ = old_tx.send(Message::Text(msg.into()));
            let _ = old_tx.send(Message::Close(None));
        }
        conns.insert(my_id.clone(), tx.clone());
    }

    println!("{my_id} connected");

    {
        let db = state.db.lock().await;
        let pending: Vec<(i64, String)> = match db
            .prepare("SELECT id, payload FROM messages WHERE to_id = ?1 AND delivered = 0")
        {
            Ok(mut stmt) => stmt
                .query_map([&my_id], |row| Ok((row.get(0)?, row.get(1)?)))
                .map(|rows| rows.filter_map(Result::ok).collect())
                .unwrap_or_default(),
            Err(e) => {
                eprintln!("failed to prepare pending-messages query for {my_id}: {e}");
                Vec::new()
            }
        };
        for (id, payload) in &pending {
            let _ = tx.send(Message::Text(payload.clone().into()));
            if let Err(e) = db.execute("UPDATE messages SET delivered = 1 WHERE id = ?1", [id]) {
                eprintln!("failed to mark message (id) delivered: {e}");
            }
        }
    }

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(msg) => {
                        if ws_sender.send(msg).await.is_err() {
                            break;
                        }
                    }
                    None => break
                }
            }

            incoming = ws_receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(parsed) => {
                                if !parsed.sender_matches(&my_id) {
                                    eprintln!("{my_id} tried to spoof sender field, dropping");
                                    continue
                                }
                                if let Some(target_id) = parsed.target() {
                                    let conns = state.connections.lock().await;
                                    if let Some(target_tx) = conns.get(target_id) {
                                        let _ = target_tx.send(Message::Text(text.clone().into())).is_ok();
                                    } else {
                                        eprintln!("{target_id} not connected, dropping message");
                                    }
                                }
                                if let ClientMessage::Message {to, from, client_id, ciphertext, nonce, self_ciphertext, self_nonce } = &parsed {
                                    const MAX_MESSAGE_BYTES: usize = 64 * 1024;
                                    if ciphertext.len() > MAX_MESSAGE_BYTES || self_ciphertext.len() > MAX_MESSAGE_BYTES {
                                        if let Some(tx) = state.connections.lock().await.get(from) {
                                            let err =  ServerMessage::Error { message: "Message too large".to_string() };
                                            let _ = tx.send(Message::Text(serde_json::to_string(&err).unwrap().into()));
                                        }
                                        continue;
                                    }

                                    let db = state.db.lock().await;
                                    let delivered = state.connections.lock().await.contains_key(to);
                                    if let Err(e) = db.execute(
                                        "INSERT INTO messages (to_id, from_id, payload, delivered) VALUES (?1,?2,?3,?4)",
                                        rusqlite::params![to, from, text, delivered as i32],
                                    ) {
                                        eprintln!("failed to insert message from {my_id}: {e}");
                                        continue;
                                    }
                                    let id = db.last_insert_rowid();

                                    drop(db);

                                    if let Some(target_tx) = state.connections.lock().await.get(to) {
                                        let incoming = ServerMessage::IncomingMessage { id, from: from.clone(), ciphertext: ciphertext.clone(), nonce: nonce.clone() };
                                        let _ = target_tx.send(Message::Text(serde_json::to_string(&incoming).unwrap().into()));
                                    }
                                    if let Some(sender_tx) = state.connections.lock().await.get(from) {
                                        let ack = ServerMessage::MessageAck { id, client_id: client_id.clone(), delivered };
                                        let _ = sender_tx.send(Message::Text(serde_json::to_string(&ack).unwrap().into()));
                                    }
                                    if !delivered {
                                        let sub_json: Option<String> = {
                                            let db = state.db.lock().await;
                                            db.query_row("SELECT push_subscription FROM users WHERE uuid = ?1", [to], |row| row.get(0)).ok()
                                        };
                                        if let Some(sub_json) = sub_json {
                                            let _ = crate::push::send_push(&sub_json, "New message", "You have a new message").await;
                                        }
                                    }
                                }
                                if let ClientMessage::FriendRequest { to_username, from } = &parsed {
                                    let db = state.db.lock().await;
                                    let to_uuid: Option<String> = db.query_row(
                                        "SELECT uuid FROM users WHERE id = ?1", [to_username], |row| row.get(0),
                                    ).ok();

                                    if let Some(to) = to_uuid {
                                        let exists: bool = db.query_row(
                                            "SELECT 1 FROM friends WHERE (requester_id = ?1 AND addressee_id = ?2) OR (requester_id = ?2 AND addressee_id = ?1)",
                                            rusqlite::params![from, to],
                                            |_| Ok(true),
                                        ).unwrap_or(false);

                                        if !exists {
                                            let _ = db.execute(
                                                "INSERT INTO friends (requester_id, addressee_id, status) VALUES (?1, ?2, 'pending')",
                                                rusqlite::params![from, to],
                                        );
                                        drop(db);
                                        send_friend_list(&state, &to).await;
                                        }
                                    }
                                }
                                if let ClientMessage::FriendResponse {from, to, accept } = &parsed {
                                    let db = state.db.lock().await;
                                    let request_exists: bool = db.query_row(
                                        "SELECT 1 FROM friends WHERE requester_id = ?1 AND addressee_id = ?2 AND status = 'pending'",
                                        rusqlite::params![from, to],
                                        |_| Ok(true),
                                    ).unwrap_or(false);

                                    if !request_exists {
                                        drop(db);
                                        continue;
                                    }

                                    if *accept {
                                        let _ = db.execute(
                                            "UPDATE friends SET status = 'accepted' WHERE requester_id = ?1 AND addressee_id = ?2",
                                            rusqlite::params![from, to],
                                        );
                                    } else {
                                        let _ = db.execute(
                                            "DELETE FROM friends WHERE requester_id = ?1 AND addressee_id = ?2",
                                            rusqlite::params![from, to],
                                        );
                                    }

                                    drop(db);

                                    send_friend_list(&state, to).await;
                                    send_friend_list(&state, from).await;
                                }
                                if let ClientMessage::FriendListRequest { user_id } = &parsed {
                                    send_friend_list(&state, user_id).await;
                                }
                                if let ClientMessage::SetAvatar { user_id, data } = &parsed{
                                    const MAX_AVATAR_BYTES: usize = 200 * 1024;
                                    if data.len() > MAX_AVATAR_BYTES {
                                        if let Some(tx) = state.connections.lock().await.get(user_id) {
                                            let err = ServerMessage::Error { message: "Avatar too large".to_string() };
                                            let _ = tx.send(Message::Text(serde_json::to_string(&err).unwrap().into()));
                                        }
                                    } else {
                                        let _ = state.db.lock().await.execute(
                                            "UPDATE users SET avatar = ?1 WHERE uuid = ?2",
                                            rusqlite::params![data, user_id],
                                        );
                                        send_friend_list(&state, user_id).await;
                                        let friend_ids: Vec<UserId> = {
                                            let db = state.db.lock().await;
                                            match db.prepare(
                                                "SELECT CASE WHEN requester_id = ?1 THEN addressee_id ELSE requester_id END
                                                FROM friends WHERE (requester_id = ?1 OR addressee_id = ?1) AND status = 'accepted'"
                                            ) {
                                                Ok(mut stmt) => stmt.query_map([user_id], |row| row.get(0))
                                                    .map(|rows| rows.filter_map(Result::ok).collect())
                                                    .unwrap_or_default(),
                                                Err(e) => {
                                                    eprintln!("failed to prepare friend_ids query for {user_id}: {e}");
                                                    Vec::new()
                                                }
                                            }
                                        };
                                        for fid in friend_ids {
                                            send_friend_list(&state, &fid).await;
                                        }
                                   }
                                }
                                if let ClientMessage::SetDisplayName { user_id, name } = &parsed {
                                    if name.trim().is_empty() || name.len() > MAX_DISPLAY_NAME_LEN {
                                        if let Some(tx) =  state.connections.lock().await.get(user_id) {
                                            let err =  ServerMessage::Error { message: "Display name must be 1-64 characters".to_string() };
                                            let _ = tx.send(Message::Text(serde_json::to_string(&err).unwrap().into()));
                                        }
                                        continue;
                                    }
                                    let _ = state.db.lock().await.execute(
                                        "UPDATE users SET display_name = ?1 WHERE uuid = ?2",
                                        rusqlite::params![name, user_id],
                                    );

                                    send_friend_list(&state, user_id).await;
                                    let friend_ids: Vec <UserId> = {
                                        let db = state.db.lock().await;
                                        match db.prepare(
                                            "SELECT CASE WHEN requester_id = ?1 THEN addressee_id ELSE requester_id END
                                            FROM friends WHERE (requester_id = ?1 OR addressee_id = ?1) AND status = 'accepted'"
                                        ) {
                                            Ok(mut stmt) => stmt.query_map([user_id], |row| row.get(0))
                                                .map(|rows| rows.filter_map(Result::ok).collect())
                                                .unwrap_or_default(),
                                            Err(e) => {
                                                eprintln!("failed to prepare friend_ids query for {user_id}: {e}");
                                                Vec::new()
                                            }
                                        }
                                    };
                                    for fid in friend_ids {
                                        send_friend_list(&state, &fid).await;
                                    }
                                }
                                if let ClientMessage::HistoryRequest { user, with } = &parsed {
                                    let db = state.db.lock().await;

                                    let is_friend: bool = db.query_row(
                                        "SELECT 1 FROM friends WHERE status = 'accepted' AND
                                        ((requester_id = ?1 AND  addressee_id = ?2) OR (requester_id = ?2 AND addressee_id = ?1))",
                                        rusqlite::params![user, with],
                                        |_| Ok(true),
                                    ).unwrap_or(false);

                                    if !is_friend {
                                        drop(db);
                                        continue;
                                    }

                                    let rows: Vec<(i64, String, String, i64)> = match db.prepare(
                                        "SELECT id, from_id, payload, read FROM messages
                                        WHERE (to_id = ?1 AND from_id = ?2) OR (to_id = ?2 AND from_id = ?1)
                                        ORDER BY id ASC"
                                    ) {
                                        Ok (mut stmt) => stmt.query_map(
                                            rusqlite::params![user, with],
                                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                                        ).map(|rows| rows.filter_map(Result::ok).collect()).unwrap_or_default(),
                                        Err(e) => {
                                            eprintln!("failed to prepare history query for {user}/{with}: {e}");
                                            continue;
                                        }
                                    };
                                    drop(db);

                                    let messages: Vec<HistoryMessage> = rows.into_iter().filter_map(|(id, from, payload, read)| {
                                        serde_json::from_str::<serde_json::Value>(&payload).ok().and_then(|v| {
                                            Some(HistoryMessage {
                                                id,
                                                from,
                                                ciphertext: v.get("ciphertext")?.as_str()?.to_string(),
                                                nonce: v.get("nonce")?.as_str()?.to_string(),
                                                self_ciphertext: v.get("self_ciphertext")?.as_str()?.to_string(),
                                                self_nonce: v.get("self_nonce")?.as_str()?.to_string(),
                                                read: read != 0,
                                            })
                                        })
                                    }).collect();

                                    let response = ServerMessage::History { with: with.clone(), messages };
                                    let msg = serde_json::to_string(&response).unwrap();
                                    if let Some(target_tx) = state.connections.lock().await.get(user) {
                                        let _ = target_tx.send(Message::Text(msg.into()));
                                    }
                                }
                                if let ClientMessage::SetPublicKey { user_id, key } = &parsed {
                                    let _ = state.db.lock().await.execute(
                                        "UPDATE users SET public_key = ?1 WHERE uuid = ?2",
                                        rusqlite::params![key, user_id],
                                    );
                                    send_friend_list(&state, user_id).await;
                                    let friend_ids: Vec<UserId> = {
                                        let db=state.db.lock().await;
                                        match db.prepare(
                                            "SELECT CASE WHEN requester_id = ?1 THEN addressee_id ELSE requester_id END
                                            FROM friends WHERE (requester_id = ?1 OR addressee_id = ?1) AND status = 'accepted'"
                                        ) {
                                            Ok(mut stmt) => stmt.query_map([user_id], |row| row.get(0))
                                                .map(|rows| rows.filter_map(Result::ok).collect())
                                                .unwrap_or_default(),
                                            Err(e) => {
                                                eprintln!("failed to prepare friend_ids query for {user_id}: {e}");
                                                Vec::new()
                                            }
                                        }
                                    };
                                    for fid in friend_ids {
                                        send_friend_list(&state, &fid).await;
                                    }
                                }
                                if let ClientMessage::ReadReceipt { reader, of } = &parsed {
                                    let db = state.db.lock().await;
                                    let ids: Vec<i64> = match db.prepare(
                                        "SELECT id FROM messages WHERE to_id = ?1 AND from_id = ?2 AND read = 0"
                                    ) {
                                        Ok(mut stmt) => stmt.query_map(rusqlite::params![reader, of], |row| row.get(0))
                                            .map(|rows| rows.filter_map(Result::ok).collect())
                                            .unwrap_or_default(),
                                        Err(e) => {
                                            eprintln!("failed to prepare read-receipt query for {reader}: {e}");
                                            Vec::new()
                                        }
                                    };
                                    if let Err(e) = db.execute(
                                        "UPDATE messages set read = 1 WHERE to_id = ?1 AND from_id = ?2 AND read = 0",
                                        rusqlite::params![reader, of],
                                    ) {
                                        eprintln!("failed to mark messages read for {reader}/{of}: {e}")
                                    }
                                    drop(db);

                                    if let Some(target_tx) = state.connections.lock().await.get(of) {
                                        let msg = ServerMessage::MessagesRead {by: reader.clone(), of: of.clone(), message_ids: ids };
                                        let _ = target_tx.send(Message::Text(serde_json::to_string(&msg).unwrap().into()));
                                    }
                                }
                                if let ClientMessage::SetPushSubscription { user_id, subscription } = &parsed {
                                    let _ = state.db.lock().await.execute(
                                        "UPDATE users SET push_subscription = ?1 WHERE uuid = ?2",
                                        rusqlite::params![subscription, user_id],
                                    );
                                }
                                if let ClientMessage::Logout { token } = &parsed {
                                    let _ = state.db.lock().await.execute(
                                        "DELETE FROM sessions WHERE token = ?1",
                                        [token],
                                    );
                                }
                            }

                            Err(e) => eprintln!("bad message from {my_id}: {e}"),
                        }
                    }
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
    state.connections.lock().await.remove(&my_id);
    println!("{my_id} disconnected");
}
