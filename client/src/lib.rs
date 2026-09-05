// client/src/lib.rs

slint::include_modules!();

#[allow(dead_code)]
mod app_state;
mod crypto;
mod keys;
mod protocol;
mod websocket;

use base64::Engine;
use protocol::ClientMessage;
use slint::Model;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use uuid::Uuid;

pub type FriendKeys = Arc<Mutex<HashMap<String, String>>>;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).unwrap();
    run_app();
}
pub fn run_app() {
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    let (secret_key, public_key) = keys::load_or_generate_keypair();
    let friend_keys: FriendKeys = Arc::new(Mutex::new(HashMap::new()));
    let own_public_key_b64 =
        base64::engine::general_purpose::STANDARD.encode(public_key.as_bytes());

    let app = AppWindow::new().unwrap();
    let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<ClientMessage>();

    let app_weak = app.as_weak();
    let outgoing_tx_for_thread = outgoing_tx.clone();
    let friend_keys_for_thread = friend_keys.clone();
    let secret_key_for_thread = secret_key.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let url = "wss://10.0.0.25:3000/ws";
            match websocket::connect(
                url,
                app_weak,
                outgoing_rx,
                outgoing_tx_for_thread,
                secret_key_for_thread,
                public_key,
                friend_keys_for_thread,
            )
            .await
            {
                Ok(_) => log::warn!("websocket task ended cleanly"),
                Err(e) => log::error!("websocket task died: {e}"),
            }
        });
    });

    let tx = outgoing_tx.clone();
    app.on_login_clicked(move |u, p| {
        log::info!("quick_login tapped: {u}");
        if let Err(e) = tx.send(ClientMessage::Login {
            username: u.to_string(),
            password: p.to_string(),
        }) {
            log::error!("send failed: {e}")
        }
    });

    let tx = outgoing_tx.clone();
    app.on_register_clicked(move |u, p| {
        log::info!("register tapped: {u}");
        if let Err(e) = tx.send(ClientMessage::Register {
            username: u.to_string(),
            password: p.to_string(),
        }) {
            log::error!("register failed: {e}");
        }
    });

    let tx = outgoing_tx.clone();
    app.on_quick_login(move |u, p| {
        log::info!("quick_login tapped: {u}");
        if let Err(e) = tx.send(ClientMessage::Login {
            username: u.to_string(),
            password: p.to_string(),
        }) {
            log::error!("send failed: {e}")
        }
    });

    let app_toggle = app.as_weak();
    app.on_toggle_clicked(move || {
        if let Some(app) = app_toggle.upgrade() {
            app.set_is_register(!app.get_is_register());
        }
    });

    let tx = outgoing_tx.clone();
    let app_weak_friend = app.as_weak();
    app.on_friend_request_response(move |from_id, accept| {
        if let Some(app) = app_weak_friend.upgrade() {
            let my_id = app.get_my_user_id().to_string();
            log::info!("friend_request_response: {from_id} accept={accept}");
            let _ = tx.send(ClientMessage::FriendResponse {
                from: from_id.to_string(),
                to: my_id,
                accept,
            });
        }
    });

    let tx = outgoing_tx.clone();
    let app_weak_chat = app.as_weak();
    app.on_open_chat(move |friend_id, friend_name| {
        log::info!("open_chat tapped: {friend_id} {friend_name}");
        if let Some(app) = app_weak_chat.upgrade() {
            let my_id = app.get_my_user_id().to_string();
            app.set_active_chat_friend_id(friend_id.clone());
            app.set_active_chat_friend_name(friend_name);
            app.set_chat_messages(
                std::rc::Rc::new(slint::VecModel::from(Vec::<MessageData>::new())).into(),
            );
            let _ = tx.send(ClientMessage::HistoryRequest {
                user: my_id.clone(),
                with: friend_id.to_string(),
            });
            let _ = tx.send(ClientMessage::ReadReceipt {
                reader: my_id,
                of: friend_id.to_string(),
            });
        }
    });

    let app_weak_close = app.as_weak();
    app.on_close_chat(move || {
        if let Some(app) = app_weak_close.upgrade() {
            app.set_active_chat_friend_id("".into());
        }
    });

    let tx = outgoing_tx.clone();
    let app_weak_send = app.as_weak();
    let secret_key_send = secret_key.clone();
    let friend_keys_send = friend_keys.clone();
    let own_public_key_send = own_public_key_b64.clone();
    app.on_send_message(move |text| {
        if let Some(app) = app_weak_send.upgrade() {
            let my_id = app.get_my_user_id().to_string();
            let friend_id = app.get_active_chat_friend_id().to_string();
            let client_id = Uuid::new_v4().to_string();

            let recipient_key = friend_keys_send.lock().unwrap().get(&friend_id).cloned();

            let Some(recipient_key) = recipient_key else {
                app.set_send_error(
                    "Can't send: no encryption key on file for this friend yet.".into(),
                );
                return;
            };

            let Some(recipient_enc) = crypto::encrypt(&secret_key_send, &recipient_key, &text)
            else {
                app.set_send_error("Encryption failed. Message was not sent.".into());
                return;
            };

            let Some(self_enc) = crypto::encrypt(&secret_key_send, &own_public_key_send, &text)
            else {
                app.set_send_error("Encryption failed. Message was not sent".into());
                return;
            };
            app.set_send_error("".into());
            app.set_msg_input_text("".into());

            let _ = tx.send(ClientMessage::Message {
                to: friend_id.clone(),
                from: my_id.clone(),
                client_id: client_id.clone(),
                ciphertext: recipient_enc.ciphertext,
                nonce: recipient_enc.nonce,
                self_ciphertext: self_enc.ciphertext,
                self_nonce: self_enc.nonce,
            });

            let model = app.get_chat_messages();
            if let Some(vec_model) = model
                .as_any()
                .downcast_ref::<slint::VecModel<MessageData>>()
            {
                vec_model.push(MessageData {
                    id: client_id.into(),
                    text: text.to_string().into(),
                    is_mine: true,
                    is_error: false,
                    delivered: false,
                    read: false,
                });
            }
        }
    });

    let app_weak_settings = app.as_weak();
    app.on_open_settings(move || {
        log::info!("settings tapped");
        // TODO: navigate to settings screen
    });

    app.run().unwrap();
}
