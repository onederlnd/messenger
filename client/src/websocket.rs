// client/src/websocket.rs

use crate::crypto;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::AppWindow;
use crate::FriendData;
use crate::FriendKeys;
use crate::MessageData;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use slint::{Model, Weak};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::Connector;
use x25519_dalek::{PublicKey, StaticSecret};

const SERVER_CERT_PEM: &str = include_str!("../../client/rootCA.pem");

fn format_relative_time(sql_datetime: &str) -> String {
    use chrono::{NaiveDateTime, Utc};
    let parsed = NaiveDateTime::parse_from_str(sql_datetime, "%Y-%m-%d %H:%M:%S");
    let Ok(dt) = parsed else {
        return String::new();
    };

    let now = Utc::now().naive_utc();
    let diff = now.signed_duration_since(dt);

    if diff.num_seconds() < 60 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{}d", diff.num_days())
    } else {
        format!("{}w", diff.num_weeks() / 7)
    }
}

pub async fn connect(
    url: &str,
    app_weak: Weak<AppWindow>,
    mut outgoing: mpsc::UnboundedReceiver<ClientMessage>,
    outgoing_tx: mpsc::UnboundedSender<ClientMessage>,
    secret_key: StaticSecret,
    public_key: PublicKey,
    friend_keys: FriendKeys,
) -> Result<(), String> {
    println!("attepting to connect to {url}");

    let mut root_store = rustls::RootCertStore::empty();
    let mut cert_reader = std::io::BufReader::new(SERVER_CERT_PEM.as_bytes());

    for cert in rustls_pemfile::certs(&mut cert_reader) {
        let cert = cert.map_err(|e| format!("cert parse error: {e}"))?;
        root_store
            .add(cert)
            .map_err(|e| format!("cert add error: {e}"))?;
    }

    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let connector = Connector::Rustls(std::sync::Arc::new(client_config));

    let request = url.into_client_request().map_err(|e| e.to_string())?;

    let (ws_stream, _) =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
            .await
            .map_err(|e| {
                println!("connection failed: {e}");
                e.to_string()
            })?;
    println!("connected to {url}");
    let (mut write, mut read) = ws_stream.split();

    loop {
        tokio::select! {
                    outgoing_msg = outgoing.recv() => {
                        match outgoing_msg {
                            Some(msg) => {
                                let json = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
                                write.send(Message::Text(json.into())).await.map_err(|e| e.to_string())?;
                            }
                            None => break,
                        }
                    }
                    incoming = read.next() => {
                        match incoming {
                            Some(Ok(Message::Text(text))) => {
                                // if let Ok(msg) = serde_json::from_str::<ServerMessage>(&text) {
                                match serde_json::from_str::<ServerMessage>(&text) {
                                    Ok(msg) => {
                                    let app_weak = app_weak.clone();
                                    match msg {
                                        ServerMessage::AuthSuccess { token, id, username, .. } => {
                                            let id_clone = id.clone();
                                            let initial = username.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                                            let app_weak2 = app_weak.clone();
                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(app) = app_weak2.upgrade() {
                                                    app.set_login_error("".into());
                                                    app.set_logged_in(true);
                                                    app.set_my_user_id (id_clone.into());
                                                    app.set_my_initial(initial.into());
                                                }
                                            });

                                            let key_b64 = base64::engine::general_purpose::STANDARD.encode(public_key.as_bytes());

                                            let _ = outgoing_tx.send(ClientMessage::SetPublicKey {
                                                user_id: id.clone(),
                                                key: key_b64,
                                            });
                                            let _ = outgoing_tx.send(ClientMessage::FriendListRequest { user_id: id.clone() });
                                        }
                                        ServerMessage::AuthError { message } => {
                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(app) = app_weak.upgrade() {
                                                    app.set_login_error(message.into());
                                                }
                                            });
                                        }
                                        ServerMessage::FriendList { friends, pending_incoming } => {
                                            {
                                                let mut keys = friend_keys.lock().unwrap();
                                                for f in &friends {
                                                    log::info!("friend {} public_key present: {}", f.id, f.public_key.is_some());
                                                    if let Some(k) = &f.public_key {
                                                        keys.insert(f.id.clone(), k.clone());
                                                    }
                                                }
                                            }
                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(app) = app_weak.upgrade() {
                                                    let friend_model: Vec<FriendData> = friends.iter().map(|f| {
                                                        let name = f.display_name.clone().filter (|s| !s.is_empty())
                                                            .or_else(|| f.username.clone())
                                                            .unwrap_or_default();
                                                        let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                                                        FriendData {
                                                            id: f.id.clone().into(),
                                                            username: f.username.clone().unwrap_or_default().into(),
                                                            display_name: f.display_name.clone().unwrap_or_default().into(),
                                                            avatar: f.avatar.clone().unwrap_or_default().into(),
                                                            initial: initial.into(),
                                                            last_message: f.last_message.clone().unwrap_or_default().into(),
                                                            last_message_time: f.last_message_time.as_deref().map(format_relative_time).unwrap_or_default().into(),
                                                        }
                                                    }).collect();

                                                    let pending_model: Vec<FriendData> = pending_incoming.iter().map(|f| {
                                                        let name = f.display_name.clone().filter (|s| !s.is_empty())
                                                            .or_else(|| f.username.clone())
                                                            .unwrap_or_default();
                                                        let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                                                        FriendData {
                                                            id: f.id.clone().into(),
                                                            username: f.username.clone().unwrap_or_default().into(),
                                                            display_name: f.display_name.clone().unwrap_or_default().into(),
                                                            avatar: f.avatar.clone().unwrap_or_default().into(),
                                                            initial: initial.into(),
                                                            last_message: f.last_message.clone().unwrap_or_default().into(),
                                                            last_message_time: f.last_message_time.as_deref().map(format_relative_time).unwrap_or_default().into(),
                                                        }
                                                    }).collect();

                                                    app.set_friends(std::rc::Rc::new(slint::VecModel::from(friend_model)).into());
                                                    app.set_pending_incoming(std::rc::Rc::new(slint::VecModel::from(pending_model)).into());
                                                }
                                            });
                                        }
                                        ServerMessage::History { messages, with } => {
                                            let my_public_b64 = base64::engine::general_purpose::STANDARD.encode(public_key.as_bytes());
                                            let friend_public = friend_keys.lock().unwrap().get(&with).cloned();
                                            let secret_key_history = secret_key.clone();

                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(app) = app_weak.upgrade() {
                                                    let my_id = app.get_my_user_id().to_string();
                                                    let msg_model: Vec<MessageData> = messages.iter().map(|m| {
                                                        let is_mine = m.from == my_id;
                                                        let decrypted = if is_mine {
                                                            crypto::decrypt(&secret_key_history, &my_public_b64, &m.self_ciphertext, &m.self_nonce)
                                                        } else {
                                                            friend_public.as_deref ()
                                                                .and_then(|k| crypto::decrypt(&secret_key_history, k, &m.ciphertext, &m.nonce))
                                                        };
                                                        let (text, is_error) = match decrypted {
                                                            Some(plaintext) => (plaintext, false),
                                                            None => ("[Message could not be decrypted]".to_string(), true),
                                                        };

                                                        MessageData {
                                                            id: m.id.to_string().into(),
                                                            text: text.into(),
                                                            is_mine,
                                                            is_error,
                                                            delivered: true,
                                                            read: m.read,
                                                        }
                                                    }).collect();
                                                    app.set_chat_messages(std::rc::Rc::new(slint::VecModel::from(msg_model)).into())
                                                }
                                            });
                                        }
                                        ServerMessage::IncomingMessage { id, from, ciphertext, nonce } => {
                                            let sender_key = friend_keys.lock().unwrap().get(&from).cloned();
                                            let (text, is_error) = match sender_key.as_deref().and_then(|k| crypto::decrypt(&secret_key, k, &ciphertext, &nonce)) {
                                                Some(plaintext) => (plaintext, false),
                                                None => ("[Message could not be decrypted]".to_string(), true),
                                            };

                                            let outgoing_tx = outgoing_tx.clone();
                                            let from2 = from.clone();

                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(app) = app_weak.upgrade() {
                                                    if app.get_active_chat_friend_id().to_string() == from {
                                                        let model = app.get_chat_messages();
                                                        if let Some(vec_model) = model.as_any().downcast_ref::<slint::VecModel<MessageData>>() {
                                                            vec_model.push(MessageData {
                                                                id: id.to_string().into(),
                                                                text: text.into(),
                                                                is_mine: false,
                                                                is_error,
                                                                delivered: true,
                                                                read: false,
                                                            });
                                                        }
                                                        let my_id = app.get_my_user_id().to_string();
                                                        let _ = outgoing_tx.send(ClientMessage::ReadReceipt {
                                                            reader: my_id,
                                                            of: from2,
                                                        });
                                                    }
                                                }
                                            });
                                        }
                                        ServerMessage::MessageAck { client_id, delivered, .. } => {
                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(app) = app_weak.upgrade() {
                                                    let model = app.get_chat_messages();
                                                    if let Some(vec_model) = model.as_any().downcast_ref::<slint::VecModel<MessageData>>() {
                                                        for i in 0..vec_model.row_count() {
                                                            let mut m = vec_model.row_data(i).unwrap();
                                                            if m.id == client_id {
                                                                m.delivered = delivered;
                                                                vec_model.set_row_data(i, m);
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                        ServerMessage::MessagesRead { message_ids, .. } => {
                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(app) = app_weak.upgrade() {
                                                    let model = app.get_chat_messages();
                                                    if let Some(vec_model) = model.as_any().downcast_ref::<slint::VecModel<MessageData>>() {
                                                        for i in 0..vec_model.row_count() {
                                                            let mut m = vec_model.row_data(i).unwrap();
                                                            if message_ids.iter().any(|id| m.id == id.to_string()) {
                                                                m.read = true;
                                                                vec_model.set_row_data(i, m);
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                        ServerMessage::SessionReplaced => {
                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(app) = app_weak.upgrade() {
                                                    app.set_logged_in(false);
                                                    app.set_login_error("Logged out: this account signed in elsewhere.".into());
                                                }
                                            });
                                        }
                                        ServerMessage::Error { message } => {
                                            log::error!("server error: {message}");
                                            // TODO: surface via a UI error field once one exists outside send_error
                                        }
                                        _ => {}
                                    }
                                }
                                Err(e) => {
                                    log::error!("failed to parse error message: {e} | raw: {text}");
                                }
                            }
                        }
                        Some(Ok(_)) => {}
                        _ => break,
                }
            }
        }
    }
    Ok(())
}
