# Messenger

A real-time messenger with a Rust WebSocket backend, a vanilla JS/HTML/CSS web frontend, and an in-progress cross-platform Rust (Slint) client. Users register accounts, add each other as friends, and message in real time, with messages persisted server-side (as ciphertext) so full conversation history is available on any device, any time.

> Working name: the server crate is called `messenger-server`. Rename it whenever you land on something better.

## Status

Currently a local/LAN prototype. The server runs on a fixed local IP with TLS via a local dev certificate (see [Known limitations](#known-limitations) for mobile trust caveats). Messages are end-to-end encrypted client-side using static X25519 keypairs (tweetnacl-js on the web client); the server only ever stores and relays ciphertext, never plaintext.

A cross-platform native client (Slint, Rust) is now scaffolded and building successfully for both desktop and Android — this will eventually replace the web frontend as the primary client, with the JS frontend kept around as a secondary target. The native client now has a working login/register/resume flow plus a basic friend list and real-time messaging UI (open a conversation, send and receive messages); it does not yet handle typing indicators, read receipts, or avatars.

## Features

- **Accounts with authentication**: usernames and passwords, hashed with Argon2 (not stored in plaintext)
- **Login rate limiting**: per-username and per-IP tracking with exponential backoff on repeated failures, persisted server-side so it survives restarts and works across multiple server instances
- **TLS (wss)**: the WebSocket connection runs over TLS using a local dev certificate
- **End-to-end encryption**: each client generates a static X25519 keypair on first use (private key never leaves the browser); messages are encrypted client-side before sending, and the server stores/relays only ciphertext. No forward secrecy yet (planned as a future Double Ratchet upgrade)
- **Friends system**: send/accept/reject friend requests; friend list updates live for both sides
- **Real-time messaging** over a single WebSocket connection per user, scoped per-friend conversation
- **Persistent message history**: every message is stored server-side and pulled fresh whenever a conversation is opened, so history survives refreshes, logouts, and new devices
- **Read receipts**: Signal-style checkmarks show sent, delivered, and read status per message, updated live
- **Offline delivery**: messages sent to a disconnected user are queued and flushed to them on reconnect
- **Single-session enforcement**: logging in from a second location kicks the previous session, with a clear notice to the user who was logged out
- **Typing indicators**: see when the person you're chatting with is actively typing
- **Unread message badges**: the friend list shows an unread count for conversations you haven't opened yet
- **WebRTC signaling support**: the protocol includes `offer`/`answer`/`ice-candidate` message types for peer-to-peer connections (e.g. voice/video), relayed server-side but not yet wired up on the frontend
- **Avatars**: users can upload a profile image (base64-encoded, stored in SQLite, capped at 200KB both client- and server-side); avatars are pushed to friends via the `friend-list` message and shown next to each name
- **Display name**: users can set a display name (1-64 characters) shown in the friend list, falling back to their username if unset
- **Settings screen**: avatar, display name, and logout live in a dedicated settings screen, opened by tapping your own avatar
- **Friend list previews**: each conversation shows the most recent message under the friend's name, similar to standard messaging apps; history is preloaded in the background when the friend list loads so previews are available without opening each chat first
- **Session expiry**: login sessions expire after 30 days server-side
- **Message-level authorization**: every action (messaging, friend requests/responses, history, profile updates) is checked against the authenticated connection's identity and, where relevant, friendship status — not just verified once at login
- **Client-side reconnect**: the web frontend automatically reconnects with exponential backoff if the WebSocket connection drops

## Messaging App Comparison

| | **Messenger (yours)** | **Signal** | **WhatsApp** | **Telegram** |
| --- | --- | --- | --- | --- |
| E2E encryption | Yes, static X25519 (no forward secrecy yet) | Yes, Signal Protocol, forward secrecy by default | Yes, Signal Protocol, forward secrecy | Not by default — only in opt-in "Secret Chats"; cloud chats are server-side encrypted, not E2E |
| Encrypted backups | N/A (server stores ciphertext directly) | Encrypted backup support | Backups encrypted client-side with a locally generated key protected by a password or 64-digit key, optionally unlocked via passkey (fingerprint/face/screen lock) | Cloud backups not E2E |
| Read receipts | Yes, live sent/delivered/read | Yes | Yes | Yes |
| Typing indicators | Yes | Yes | Yes | Yes |
| Disappearing messages | No | Yes, with a settable default timer applied to all new chats | Yes | Yes |
| Usernames (no phone/identity leak) | Usernames used for login only, decoupled internally via UUID | Yes — a username isn't your display name or visible to existing contacts; exists only to let someone start a chat without exposing your phone number | Phone-number based | Username-based |
| Group chats | Not yet (roadmap) | Yes, up to 1,000 members | Yes | Yes, large groups/channels |
| Multi-device | Single-session enforced (by design, for now) | Yes | Yes, with hardened encryption guarantees when logged in on phone + desktop simultaneously | Yes |
| Offline delivery | Yes, queued + flushed on reconnect | Yes | Yes | Yes |
| Voice/video calls | Signaling wired server-side, not implemented | Yes, E2E | Yes, E2E | Yes (not E2E by default) |
| Open source | Fully open source | Fully open source, protocol audited | Client not open source (protocol is) | Client partially open, server closed |

**Where this project stands out even at prototype stage:**

- Message-level authorization checked on every action (not just at login)
- UUID-decoupled identity model — usernames can change later without breaking data

**Honest gaps vs. Signal:**

- Forward secrecy (already on the roadmap)
- Disappearing messages (not yet on the roadmap — a relatively small addition: a `delete-after` timestamp per message plus a client-side timer)

## Architecture

```
web frontend (HTML/JS)        <-- WebSocket (wss) -->
native client (Rust/Slint)    <-- WebSocket (wss) -->   axum server  <-->  SQLite (messages.db)
```

This is a Cargo workspace with two Rust crates, a static web frontend, and a set of dev tooling scripts:

```
server/          # axum WebSocket backend
client/          # cross-platform Rust client (Slint) — desktop + Android scaffolded, iOS pending
js_frontend/     # vanilla HTML/JS/CSS web client
scripts/         # Python dev tooling: environment launcher + git workflow helpers
```

- **Backend**: Rust, [axum](https://github.com/tokio-rs/axum) for the HTTP/WebSocket server, [axum-server](https://github.com/programatik29/axum-server) for TLS, [rusqlite](https://github.com/rusqlite/rusqlite) (bundled SQLite) for persistence, [tokio](https://tokio.rs/) as the async runtime, [argon2](https://docs.rs/argon2) for password hashing.
- **Web frontend**: plain HTML/CSS/JS, no build step or framework. Encryption uses [tweetnacl-js](https://github.com/dchest/tweetnacl-js) and tweetnacl-util, loaded via CDN.
- **Native client**: [Slint](https://slint.dev/) targeting desktop and mobile from one Rust codebase. Scaffolded and building on desktop (Linux) and Android (via emulator); iOS not yet attempted. It embeds its own trusted root CA (`rootCA.pem`) directly in the client, so — unlike the web frontend — it doesn't depend on the OS/browser trust store to accept the local dev TLS certificate. Has a working WebSocket connection, auth flow (login/register/resume), a basic friend list + messaging UI, and client-side encryption on send/receive; typing indicators, read receipts, and avatars are not yet wired up on this client. Next up is a `libsignal-protocol`-based Double Ratchet implementation (this client is intended to carry the forward-secrecy upgrade, rather than retrofitting the web client's static-key scheme).
- **Connections** are held in an in-memory `HashMap<UserId, Sender>` guarded by a `tokio::sync::Mutex`, shared across all sockets via `AppState`.
- **Rate limiting** state is persisted in a `login_attempts` SQLite table (failure count + lockout expiry per key), keyed separately by username and by client IP, so it survives server restarts and is consistent across multiple server instances.
- **Encryption keys**: each client's private key lives only in local client-side storage; public keys are stored server-side (`users.public_key`) and distributed to friends via the `friend-list` message, the same pattern used for avatar and display name. Each message is encrypted twice client-side: once to the recipient's public key, once to the sender's own public key, so a sender's own message history remains readable after reload.
- **Read receipts**: the server assigns each message a numeric ID on insert. Delivery and read status are tracked per-message (`messages.delivered`, `messages.read`) and pushed to the sender live via dedicated `message-ack` and `messages-read` messages.
- **Identity**: each user has a stable internal UUID (`users.uuid`) used for all routing, session, and friend-graph lookups; the human-chosen username (`users.id`) is used only for login and display, and is not referenced by any other table's foreign keys — so usernames can be safely changed in the future without breaking existing relationships (no rename feature is built yet).
- **Sessions**: login tokens are stored server-side with an expiry (`sessions.expires_at`, 30 days from issue) and are cleaned up lazily on login/register.

### Project structure

```
server/
├── src/
│   ├── main.rs        # main(), router setup, TLS binding, ws_handler, handle_socket
│   ├── state.rs        # AppState, Tx type alias
│   ├── models.rs         # UserId, ClientMessage, ServerMessage, HistoryMessage, FriendInfo
│   ├── auth.rs             # password hashing, session token generation
│   ├── handlers.rs            # send_friend_list and related helpers
│   ├── rate_limit.rs             # DB-backed login attempt tracking with exponential backoff
│   └── push.rs                     # web push notification plumbing
├── Cargo.toml
├── localhost.pem / localhost-key.pem    # local dev TLS cert
├── vapid_public.pem / vapid_private.pem # web push keys
└── messages.db                           # SQLite DB (created on first run)

client/
├── src/
│   ├── lib.rs           # crate entry point
│   ├── app_state.rs      # client-side app state
│   ├── crypto.rs           # X25519 encrypt/decrypt
│   ├── keys.rs               # keypair load/generate
│   ├── protocol.rs             # message types shared with the server protocol
│   └── websocket.rs               # WebSocket connection handling
├── ui/
│   └── login.slint    # Slint UI markup (login screen, etc.)
├── build.rs
├── rootCA.pem       # embedded trusted root CA for TLS
└── Cargo.toml

js_frontend/
├── index.html
├── app.js
├── style.css
└── sw.js               # service worker for web push

scripts/
├── dev.py           # server + web + emulator launcher
├── gitflow.py         # feature/ship/close via gh CLI
└── devconfig.toml       # optional config overrides (not committed by default)
```

### Message protocol

Every message is JSON with a `type` field that determines its shape.

**Client to server:**

| Type | Fields | Purpose |
| --- | --- | --- |
| `login` | `username`, `password` | Authenticates and registers the connection |
| `register` | `username`, `password` | Creates a new account (3-32 char username, letters/numbers/`_`/`-` only; 8-256 char password), then logs in |
| `resume` | `token` | Re-authenticates an existing session token (skips password entry) |
| `logout` | `token` | Invalidates a session token server-side |
| `message` | `to`, `from`, `ciphertext`, `nonce`, `self_ciphertext`, `self_nonce`, `client_id` | An encrypted chat message (max 64KB per ciphertext field): one ciphertext for the recipient, one for the sender's own record; `client_id` lets the sender match this message to its later `message-ack` |
| `read-receipt` | `reader`, `of` | Marks all unread messages from `of` as read |
| `friend-request` | `to_username`, `from` | Send a friend request |
| `friend-response` | `from`, `to`, `accept` | Accept or reject a pending friend request |
| `friend-list-request` | `user_id` | Ask the server for the current friends list plus pending incoming requests |
| `set-avatar` | `user_id`, `data` | Set your avatar (base64 data URL, max 200KB) |
| `set-display-name` | `user_id`, `name` | Set your display name (1-64 characters) |
| `set-public-key` | `user_id`, `key` | Set your X25519 public key (base64) |
| `set-push-subscription` | `user_id`, `subscription` | Register a web push subscription for offline notifications |
| `history-request` | `user`, `with` | Ask the server for the full message history with a specific friend (requires an accepted friendship) |
| `typing` | `to`, `from` | Notify the recipient that you're typing |
| `offer` / `answer` / `ice-candidate` | `to`, `from`, `sdp`/`candidate` | WebRTC signaling (relayed only) |

**Server to client:**

| Type | Fields | Purpose |
| --- | --- | --- |
| `auth-success` | `token`, `id`, `username` | Login, registration, or session resume succeeded |
| `auth-error` | `message` | Login/registration/resume failed, validation failed, or the account is temporarily rate-limited |
| `session-replaced` | — | This session was logged out because the account logged in elsewhere |
| `friend-list` | `friends`, `pending_incoming` | Current friends (each with `id`, `username`, `avatar`, `display_name`, `public_key`) and incoming pending requests; pushed proactively whenever it changes, not just on request |
| `history` | `with`, `messages` | Full message history with a given friend; each message includes `id`, `ciphertext`/`nonce`, `self_ciphertext`/`self_nonce`, and `read` status |
| `incoming-message` | `id`, `from`, `ciphertext`, `nonce` | A new encrypted message from a friend |
| `message-ack` | `id`, `delivered`, `client_id` | Confirms a sent message was stored, with whether the recipient was online to receive it immediately; `client_id` echoes the value from the originating `message` so the sender can match ack to message |
| `messages-read` | `by`, `of`, `message_ids` | Tells the sender that a batch of their sent messages was just read |
| `error` | `message` | A non-fatal request-specific error (e.g. avatar/message too large) |

Every client message's claimed sender identity is verified against the authenticated connection before it's processed; mismatches are silently dropped. Messages are stored server-side as ciphertext with delivery/read status; offline recipients get their queued messages replayed (and marked delivered) the next time they connect.

## Getting started

### Prerequisites

- Rust (2024 edition toolchain)
- A modern browser (for the web frontend)
- A local TLS certificate (e.g. via [mkcert](https://github.com/FiloSottile/mkcert)) covering `localhost` and/or your LAN IP, saved as `localhost.pem` / `localhost-key.pem` in `server/`
- For the native client: [Slint](https://slint.dev/) tooling and a working Rust Android toolchain (`cargo-apk`); for Android builds, Android Studio with SDK, NDK, and a configured emulator, plus `ANDROID_HOME`, `ANDROID_NDK_HOME`, and `JAVA_HOME` set in your environment
- Python 3.11+ (for the dev scripts in `scripts/`)
- [GitHub CLI](https://cli.github.com/) (`gh`), installed and authenticated (for the git workflow scripts)

### Quick start (recommended)

Once the emulator AVD exists (see one-time setup below), boot the whole dev environment with one command:

```bash
python3 scripts/dev.py
```

This starts the server, the web frontend, and the emulator together, with logs prefixed and streamed inline. Ctrl+C stops everything cleanly. Flags: `--no-server`, `--no-web`, `--no-emulator`, and `--client` (also builds/runs the native client once the emulator is up). See `scripts/dev.py` for full config (paths, AVD name, commands), overridable via `scripts/devconfig.toml`.

### One-time emulator setup

```bash
# List available system images you have installed
sdkmanager --list_installed

# Install one if needed, e.g. API 35, Google APIs, x86_64
sdkmanager "system-images;android-35;google_apis_playstore;x86_64"

# Create the AVD using the Pixel 8 Pro device profile
avdmanager create avd \
  -n Pixel_8_Pro \
  -k "system-images;android-35;google_apis_playstore;x86_64" \
  -d "pixel_8_pro"
```

You only need to do this once — after that, `scripts/dev.py` launches the existing AVD by name.

### Running pieces individually

If you'd rather not use `dev.py`, each piece can still be started by hand:

```bash
# Server
cd server
cargo run

# Web frontend
cd js_frontend
npx serve .

# Emulator
emulator -avd Pixel_8_Pro

# Native client (once the emulator is running)
cd client
cargo apk run --target x86_64-linux-android --lib
```

> **Note:** `app.js` derives the WebSocket URL from the page's own hostname, so serve the web frontend from the same host as (or a host that can reach) your running server.

Once authenticated in the web frontend, tap the `+` button to add a friend by username, accept incoming requests, and click a friend to open a conversation. The native client currently has a working login/register/resume flow, friend list, and basic messaging UI; typing indicators, read receipts, and avatars aren't wired up on it yet.

## Development scripts

`scripts/` holds two Python helpers:

- **`dev.py`** — launches server + web frontend + emulator (optionally the native client) as one supervised process group; see Quick start above.
- **`gitflow.py`** — a thin GitHub-flow wrapper over the `gh` CLI:

  ```bash
  python scripts/gitflow.py feature <name>   # branch off main, push
  python scripts/gitflow.py ship [--draft]   # push + open a PR
  python scripts/gitflow.py close            # after merge: clean up local + remote branch
  ```

  `close` checks the PR's merge state via `gh pr view` before deleting anything, so it won't discard unmerged work.

## Known limitations

- **No forward secrecy**: encryption uses static X25519 keypairs, not a ratcheting scheme. If a private key is ever compromised, all past messages encrypted to it are exposed. A Double Ratchet upgrade (via `libsignal-protocol`) is planned for the native client before a wider mobile rollout.
- **Mobile TLS trust is manual for the web frontend**: the local dev certificate isn't trusted by mobile browsers unless the certificate authority is separately installed and trusted on that device; a self-signed cert accepted via browser warning is not sufficient for WebSocket (wss) connections, which fail silently rather than prompting. (The native client sidesteps this by embedding `rootCA.pem` directly, so it doesn't rely on device-level trust.)
- **Encrypted history predates encryption**: messages sent before end-to-end encryption was added are stored in a different (plaintext) shape and won't decrypt or appear in history.
- **WebRTC signaling is wired server-side but not used by either frontend**: `offer`/`answer`/`ice-candidate` messages are relayed but there's no corresponding client-side WebRTC logic yet.
- **Single-process only**: connection state lives in memory, so this won't scale across multiple server instances without a shared pub/sub layer (e.g. Redis). (Rate limiting was moved to SQLite and no longer has this limitation.)
- **No username-change feature**: the data model supports it (usernames aren't used as foreign keys anywhere), but there's no message type or UI to actually rename an account yet.
- **Native client is missing typing indicators, read receipts, and avatar handling**: builds and runs on desktop and Android with a working auth flow, encrypted messaging, and basic UI, but these features from the web frontend aren't ported over yet.

## Roadmap

**1.0** — E2E messenger ships (dev shortcuts out, real host, real TLS cert, read receipts wired, per-message rate limiting)
**1.x** — typing indicators, avatars, settings screen, unread badges, friend-list previews, push notifications (APNs/FCM), iOS build, username change

**2.0** — Double Ratchet (forward secrecy)
**2.x** — GIF support, message editing, message deletion, reactions, media/file attachments (chunked encryption, thumbnails)

**3.0** — Multi-device support
**3.x** — Encrypted backups, disappearing messages

**4.0** — Group chats
**4.x** — Voice/video calls, blocking/reporting

**5.0** — SMS tab (Android-only, default-handler)
**5.x** — Account deletion / data export

#### 1.0 (launch bar)

- [x] Rate limiting / brute-force protection on login attempts (DB-backed, persists across restarts and works across multiple server instances)
- [x] Avatar support (base64-encoded image stored in `users.avatar`, included in friend-list pushes)
- [x] Display name (stored in `users.display_name`, included in friend-list pushes)
- [x] TLS for the WebSocket connection (local dev certificate)
- [x] End-to-end encryption, static-key phase (X25519 via tweetnacl-js; server stores/relays ciphertext only)
- [x] Read receipts (sent/delivered/read status per message, live updates)
- [x] Message-level authorization (server verifies sender identity, friendship, on every action — not just at login)
- [x] Session expiry
- [x] Message size limits
- [x] Client-side WebSocket reconnect with backoff (web frontend)
- [x] Internal user IDs decoupled from username
- [x] Replace remaining `.unwrap()`s on DB calls with proper error handling
- [x] Persistent (non-in-memory) rate limiting, so it survives restarts and works across multiple server instances
- [x] Input validation - username/password (Register)
- [x] Input validation - display name length
- [x] Native client toolchain scaffolded (Slint workspace crate; building on desktop and Android)
- [x] Native client: WebSocket connection + auth flow (login/register/resume)
- [x] Native client: friend list (view friends, send/accept/reject requests)
- [x] Native client: open a conversation, send and receive messages
- [x] Native client: generate X25519 keypair on first launch, store private key as a plain file in app-private storage (upgrade to OS keystore via `keyring` later)
- [x] Native client: encrypt outgoing messages (recipient + self ciphertext) before send
- [x] Native client: decrypt incoming messages and history
- [x] Native client: receive message-ack / messages-read, delivered/read status in message UI
- [ ] Native client: send read-receipts (mark-as-read on open conversation)
- [ ] Remove dev-only shortcuts (`on_quick_login`)
- [ ] Move server off hardcoded LAN IP to a reachable host
- [ ] Properly trusted (non-dev) TLS certificate
- [ ] Per-message rate limiting (send/friend-request abuse, not just login)

### 1.x

- [ ] Typing indicators
- [ ] Avatar upload + display in friend list (native client)
- [ ] Display name editing
- [ ] Settings screen (avatar, display name, logout)
- [ ] Friend list message previews
- [ ] Unread message badges
- [ ] Single-session enforcement handling (`session-replaced` notice)
- [ ] Offline delivery handling / queued message replay on reconnect
- [ ] Push notifications: APNs/FCM backend + client-side (silent-push + local decrypt, no plaintext in payload)
- [ ] iOS build target
- [ ] Username-change feature

### 2.0

- [ ] Signal-style Double Ratchet upgrade for forward secrecy (via `libsignal-protocol`)

### 2.x

- [ ] GIF support (picker + inline render)
- [ ] Message editing
- [ ] Message deletion
- [ ] Reactions
- [ ] Media/file attachments (chunked encryption, thumbnails, size policy)

### 3.0

- [ ] Multi-device support

### 3.x

- [ ] Encrypted backups
- [ ] Disappearing messages

### 4.0

- [ ] Group chats

### 4.x

- [ ] Wire up WebRTC signaling on a frontend (voice/video calls)
- [ ] Blocking / reporting
- [ ] Multi-instance scaling (Redis pub/sub for connection state)

### 5.0

- [ ] SMS tab (Android-only, default SMS handler)

### 5.x

- [ ] Account deletion / data export

## License

TBD