let myId;
let ws;
let reconnectDelay = 1000;
let myUsername;
let peerId;
let authMode = "login"
let typingTimeout;
let lastTypingSent = 0;
let currentFriends = [];
let currentPendingIncoming = [];
let pendingSentEl = null;

// Message window
const messageInput = document.getElementById("messageInput");
const composer = document.getElementById("composer");
const messagesList = document.getElementById("messages");
const statusDot = document.getElementById("statusDot");
const statusText = document.getElementById("statusText");
const myIdDisplay = document.getElementById("myIdDisplay");

// Auth
const authScreen = document.getElementById("authScreen");
const chatScreen = document.getElementById("chatScreen");
const authForm = document.getElementById("authForm");
const authUsername = document.getElementById("authUsername");
const authPassword = document.getElementById("authPassword");
const authError = document.getElementById("authError");
const authTitle = document.getElementById("authTitle");
const authSubmitBtn = document.getElementById("authSubmitBtn");
const authToggleBtn = document.getElementById("authToggleBtn");
const authToggleText = document.getElementById("authToggleText");
const logoutBtn = document.getElementById("logoutBtn");

// Friend list
const addFriendForm = document.getElementById("addFriendForm"); 
const addFriendInput = document.getElementById("addFriendInput");
const pendingRequests = document.getElementById("pendingRequests");
const friendsList = document.getElementById("friendsList");
const chatThread = document.getElementById("chatThread");
const backToFriends = document.getElementById("backToFriends");
const avatarInput = document.getElementById("avatarInput");
const addFriendFab = document.getElementById("addFriendFab");
const addFriendPopup =  document.getElementById("addFriendPopup");

const settingsScreen = document.getElementById("settingsScreen"); 
const backFromSettings = document.getElementById("backFromSettings");
const settingsAvatarImg = document.getElementById("settingsAvatarImg");
const headerAvatarImg = document.getElementById("myAvatarImg");

const conversations = new Map();
const unreadCounts = new Map();
const lastMessages = new Map();
const friendPublicKeys = new Map();
const sentMessageEls = new Map();

const loadedHistory = new Set();

const VAPID_PUBLIC_KEY = "9zIsVS88i9fruZX35TQreEbsOVaJptVF_Yr9cUZW_dU";
const MAX_RECONNECT_DELAY = 30000;

const WS_HOST = window.location.hostname || "10.0.0.25";
const WS_PORT = 3000;
const WS_URL = `wss://${WS_HOST}:${WS_PORT}/ws`;

function connect() {
    ws = new WebSocket(WS_URL);
    
    ws.onopen = () => {
        statusText.textContent = "Connected";
        reconnectDelay = 1000;
        const savedToken = localStorage.getItem("sessionToken");
        if (savedToken) {
            safeSend({ type: "resume", token: savedToken });
        }
    };

    ws.onmessage = (event) => {
        const msg = JSON.parse(event.data);

        if (msg.type === "auth-success") {
            myId = msg.id;
            myUsername = msg.username;
            localStorage.setItem("sessionToken", msg.token);
            myIdDisplay.textContent = myUsername;
            authScreen.hidden = true;
            chatScreen.hidden = false;
            statusText.textContent = "";

            ensureKeypair();

            safeSend({ type: "friend-list-request", user_id: myId });
        } else if (msg.type === "auth-error") {
            localStorage.removeItem("sessionToken");
            authError.textContent = msg.message;
            authError.hidden = false;
        } else if (msg.type === "message") {
            const senderKeyB64 = friendPublicKeys.get(msg.from);
            const mySecretKey = nacl.util.decodeBase64(localStorage.getItem("secretKey"));
            if (senderKeyB64) {
                const senderKey = nacl.util.decodeBase64(senderKeyB64);
                const nonce = nacl.util.decodeBase64(msg.nonce);
                const ciphertext = nacl.util.decodeBase64(msg.ciphertext);
                const decrypted = nacl.box.open(ciphertext, nonce, senderKey, mySecretKey);
                const text = decrypted ? nacl.util.encodeUTF8(decrypted) : "[unable to decrypt]"
                pushMessage(msg.from, text, "received");
            }
        } else if (msg.type === "session-replaced") {
            statusText.textContent = "Logged out - logged in from another location";
            chatScreen.hidden = true;
            authScreen.hidden = false;
        } else if (msg.type === "friend-list") {
            currentFriends = msg.friends;
            currentPendingIncoming = msg.pending_incoming; 
            msg.friends.forEach(f => {
                if (f.public_key) friendPublicKeys.set(f.id, f.public_key);
            });
            renderFriendList(msg.friends, msg.pending_incoming);

            msg.friends.forEach((friend) => {
                if (!loadedHistory.has(friend.id)) {
                    loadedHistory.add(friend.id);
                    safeSend({ type: "history-request", user: myId, with: friend.id})
                }
            });
        } else if (msg.type === "history") {
            const mySecretKey = nacl.util.decodeBase64(localStorage.getItem("secretKey"));
            const decryptOne = (m) => {
                if (m.from === myId) {
                    const myPublicKey = nacl.util.decodeBase64(localStorage.getItem("publicKey"));
                    const nonce = nacl.util.decodeBase64(m.self_nonce);
                    const ciphertext = nacl.util.decodeBase64(m.self_ciphertext);
                    const decrypted = nacl.box.open(ciphertext, nonce, myPublicKey, mySecretKey);
                    return decrypted ? nacl.util.encodeUTF8(decrypted) : "[unable to decrypt]";
                }
                const senderKeyB64 = friendPublicKeys.get(m.from);
                if (!senderKeyB64) return "[unable to decrypt]";
                const senderKey = nacl.util.decodeBase64(senderKeyB64);
                const nonce = nacl.util.decodeBase64(m.nonce);
                const ciphertext = nacl.util.decodeBase64(m.ciphertext);
                const decrypted = nacl.box.open(ciphertext,nonce, senderKey, mySecretKey);
                return decrypted ? nacl.util.encodeUTF8(decrypted) : "[unable to decrypt]";
            };

            if (msg.with === peerId) {
                msg.messages.forEach(m => {
                    const kind = m.from === myId ? "sent" : "received";
                    pushMessage(msg.with, decryptOne(m), kind);
                });
            } else {
                // background preload for friend list message preview
                conversations.set(msg.with, msg.messages.map(m => ({
                    text: decryptOne(m),
                    kind: m.from === myId ? "sent" : "received",
                })));

                if (msg.messages.length > 0) {
                    const last = msg.messages[msg.messages.length - 1];
                    lastMessages.set(msg.with, {
                        text: decryptOne(last),
                        kind: last.from === myId ? "sent": "received"
                    });
                }
                renderFriendList(currentFriends, currentPendingIncoming);
            }
        } else if (msg.type === "typing") {
            if (msg.from === peerId) {
                statusText.textContent = `${msg.from} is typing...`;
                clearTimeout(typingTimeout);
                typingTimeout = setTimeout(() => {
                    statusText.textContent = `Chatting with ${peerId}`;
                }, 3000);
            }
        } else if (msg.type === "message-ack"){
            if (pendingSentEl) {
                pendingSentEl.dataset.id = msg.id;
                sentMessageEls.set(msg.id, pendingSentEl);
                const check = pendingSentEl.querySelector(".receipt-icon");
                if (check) check.textContent = msg.delivered ? "✓" : "✓✓"
                pendingSentEl = null;
            }
        } else if (msg.type === "incoming-message") {
            const senderKeyB64 = friendPublicKeys.get(msg.from);
            const mySecretKey = nacl.util.decodeBase64(localStorage.getItem("secretKey"));
            if (senderKeyB64) {
                const senderKey = nacl.util.decodeBase64(senderKeyB64);
                const nonce = nacl.util.decodeBase64(msg.nonce);
                const ciphertext = nacl.util.decodeBase64(msg.ciphertext);
                const decrypted = nacl.box.open(ciphertext, nonce, senderKey, mySecretKey);
                const text = decrypted ? nacl.util.encodeUTF8(decrypted) : "[unable to decrypt]";
                pushMessage(msg.from, text, "received");
                if (peerId === msg.from) {
                    safeSend({ type: "read-receipt", reader: myId, of: msg.from });
                }
            }
        } else if (msg.type === "messages-read") {
            msg.message_ids.forEach(id => {
                const el = sentMessageEls.get(id);
                if (el) {
                    const check = el.querySelector(".receipt-icon");
                    if (check) {
                        check.textContent = "✓✓";
                        check.classList.add("read");
                    }
                }
            });
        } else if (msg.type === "error") {
            alert(msg.message);
        }
    };

    ws.onclose = () => {
        statusText.textContent = "Disconnected - reconnecting...";
        setTimeout(connect, reconnectDelay);
        reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY);
    };

    ws.onerror = () => {
        ws.close()
    }
}
connect();

function addMessage(text, kind, id, status) {
    const li = document.createElement("li");
    li.className = kind;
    li.textContent = text;
    if (id != null) li.dataset.id = id;

    if (kind === "sent") {
        const check = document.createElement("span");
        check.className = "receipt-icon";
        check.textContent = status === "read" ? "✓✓" : status === "delivered" ? "✓✓" : "✓";
        if (status === "read") check.classList.add("read");
        li.appendChild(check);
    }
    messagesList.appendChild(li);
    messagesList.scrollTop = messagesList.scrollHeight;
    return li;
}

function pushMessage(friendId, text, kind) {
    if (!conversations.has(friendId)) {
        conversations.set(friendId, []);
    }

    conversations.get(friendId).push({ text, kind });
    lastMessages.set(friendId, { text, kind });

    if (peerId === friendId) {
        addMessage(text, kind);
    } else if (kind === "received") {
        unreadCounts.set(friendId, (unreadCounts.get(friendId) || 0) + 1);
    }
    renderFriendList(currentFriends, currentPendingIncoming);
}

function safeSend(payload){
    if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(payload));
    } else {
        console.warn("WebSocket not open, dropping", payload);
    }
}

function setActivePeer(id){
    peerId = id;
    statusText.textContent = id;
}
function urlBase64ToUint8Array(base64String) {
    const padding = "=".repeat((4 - base64String.length % 4) % 4);
    const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
    const rawData = atob(base64);
    return Uint8Array.from([...rawData].map(c => c.charCodeAt(0)));
}

function updateAuthUI(){
    if (authMode === "login") {
        authTitle.textContent = "Log in";
        authSubmitBtn.textContent = "Log in";
        authToggleText.textContent = "Don't have an account?";
        authToggleBtn.textContent = "Register";
    } else {
        authTitle.textContent = "Register";
        authSubmitBtn.textContent = "Register";
        authToggleText.textContent = "Already have an account?";
        authToggleBtn.textContent = "Log in";
    }
    authError.hidden = true;
}

function openChatWith(friendId) {
    setActivePeer(friendId);
    messagesList.innerHTML = "";
    unreadCounts.delete(friendId);
    renderFriendList(currentFriends, currentPendingIncoming);
    
    const cached = conversations.get(friendId);
    if (cached && cached.length > 0) {
        cached.forEach(m => addMessage(m.text, m.kind));
    } else {
        safeSend({ type: "history-request", user: myId, with: friendId });
    }

    safeSend({ type: "read-receipt", reader: myId, of: friendId });
    
    friendsList.parentElement.querySelectorAll("#addFriendForm, #pendingRequests, #friendsList").forEach(el => el.hidden = true);
    chatThread.hidden = false;
    addFriendFab.hidden = true;
    addFriendPopup.hidden = true;
}

function renderFriendList(friends, pendingIncoming) {
    friendsList.innerHTML = "";
    friends.forEach((friend) => {
        const li = document.createElement("li");
        li.className = "friend-item";

        const avatarImg = document.createElement("img");
        avatarImg.className = "friend-avatar"
        avatarImg.src = friend.avatar || "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='36' height='36'%3E%3Ccircle cx='18' cy='18' r='18' fill='%232a2e38'/%3E%3C/svg%3E";
        avatarImg.alt = friend.display_name || friend.username || "friend";
        li.appendChild(avatarImg);

        const textWrap = document.createElement("div");
        textWrap.className = "friend-text";

        const nameSpan = document.createElement("span");
        nameSpan.className = "friend-name";
        nameSpan.textContent = friend.display_name || friend.username || "friend";
        textWrap.appendChild(nameSpan);
        
        const last = lastMessages.get(friend.id);
        const previewSpan = document.createElement("span");
        previewSpan.className = "friend-preview";
        if (last) {
            previewSpan.textContent = (last.kind === "sent" ? "You: " : "") + last.text;
        } else {
            previewSpan.textContent = "No messages yet";
        }
        textWrap.appendChild(previewSpan);

        li.appendChild(textWrap);

        const count = unreadCounts.get(friend.id) || 0;
        if (count > 0) {
            const badge = document.createElement("span");
            badge.className = "unread-badge";
            badge.textContent = count;
            li.appendChild(badge);
        }
        li.addEventListener("click", () => openChatWith(friend.id));
        friendsList.appendChild(li);
    });

    pendingRequests.innerHTML = "";
    pendingIncoming.forEach((requesterInfo) => {
        const div = document.createElement("div");
        div.className = "request-item";
        div.innerHTML = `
        <span>${requesterInfo.display_name || requesterInfo.username || "Someone"} wants to be friends</span>
        <div class="request-actions"></div>
        `;
        const actions = div.querySelector(".request-actions");

        const acceptBtn = document.createElement("button");
        acceptBtn.className = "accept-btn";
        acceptBtn.textContent = "Accept";
        acceptBtn.addEventListener("click", () => respondToRequest(requesterInfo, true));

        const rejectBtn = document.createElement("button");
        rejectBtn.className = "reject-btn";
        rejectBtn.textContent = "Reject";
        rejectBtn.addEventListener("click", () => respondToRequest(requesterInfo, false));

        actions.appendChild(acceptBtn);
        actions.appendChild(rejectBtn);
        pendingRequests.appendChild(div);
    });
}

function respondToRequest(requesterInfo, accept) {
    safeSend({ type: "friend-response", from: myId, to: requesterInfo.id, accept });
    safeSend({ type: "friend-list-request", user_id: myId })
}

function ensureKeypair() {
    let secretKeyB64 = localStorage.getItem("secretKey")
    if (!secretKeyB64) {
        const keypair = nacl.box.keyPair();
        secretKeyB64 = nacl.util.encodeBase64(keypair.secretKey);
        const publicKeyB64 = nacl.util.encodeBase64(keypair.publicKey);
        localStorage.setItem("secretKey", secretKeyB64);
        localStorage.setItem("publicKey", publicKeyB64);
        safeSend({ type: "set-public-key", user_id: myId, key: publicKeyB64 });
    }
}

document.getElementById("displayNameInput").addEventListener("change", (e) => {
    const name = e.target.value.trim();
    if (!name) return;
    safeSend({ type: "set-display-name", user_id: myId, name });
});

document.getElementById("notifToggle").addEventListener("change", async (e) => {
    if (e.target.checked) {
        const permission = await Notification.requestPermission();
        if (permission !== "granted") {
            e.target.checked = false;
            return;
        }
        const reg = await navigator.serviceWorker.register("sw.js");
        const sub = await reg.pushManager.subscribe({
            userVisibleOnly: true,
            applicationServerKey: urlBase64ToUint8Array(VAPID_PUBLIC_KEY),
        });
        safeSend({
            type: "set-push-subscription",
            user_id: myId,
            subscription: JSON.stringify(sub),
        });
    } else {
        const reg = await navigator.serviceWorker.getRegistration();
        const sub = reg && await reg.pushManager.getSubscription();
        if (sub) await sub.unsubscribe();
    }
});

addFriendForm.addEventListener("submit", (e) => {
    e.preventDefault();
    const targetId = addFriendInput.value.trim();
    if (!targetId || targetId === myUsername) return;

    safeSend({ type: "friend-request", to_username: targetId, from: myId });
    addFriendInput.value = "";
    addFriendPopup.hidden = true;
});

addFriendFab.addEventListener("click", () => {
    addFriendPopup.hidden = !addFriendPopup.hidden;
    if (!addFriendPopup.hidden) addFriendInput.focus();
})
authToggleBtn.addEventListener("click", () => {
    authMode = authMode === "login" ? "register" : "login";
    updateAuthUI();
});

authForm.addEventListener("submit", (e) => {
    e.preventDefault();
    const username = authUsername.value.trim();
    const password = authPassword.value;
    if (!username || !password) return;

    myId = username;
    safeSend({ type: authMode, username, password});
});

avatarInput.addEventListener("change", async (e) => {
    const file = e.target.files[0];
    if (!file) return;

    const MAX_BYTES = 200*1024;
    if (file.size > MAX_BYTES) {
        alert("Image too large, please pick something smaller.");
        return;
    }

    const dataUrl = await new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result);
        reader.onerror = reject;
        reader.readAsDataURL(file);
    });
    headerAvatarImg.src = dataUrl;
    settingsAvatarImg.src = dataUrl;
    safeSend({ type: "set-avatar", user_id: myId, data: dataUrl });
});

backFromSettings.addEventListener("click", () => {
    settingsScreen.hidden = true;
    chatScreen.hidden = false;
});

backToFriends.addEventListener("click", () => {
    chatThread.hidden = true;
    addFriendForm.hidden = false;
    pendingRequests.hidden = false;
    friendsList.hidden = false;
    addFriendFab.hidden = false;
    peerId = null;
    statusText.textContent = "";
});

composer.addEventListener("submit", (e) => {
    e.preventDefault();
    const text = messageInput.value.trim();
    if (!text || !peerId) return;

    const recipientKeyB64 = friendPublicKeys.get(peerId);
    if (!recipientKeyB64) {
        alert("Can't send yet, waiting on this friends encryption key.");
        return;
    }

    const mySecretKey = nacl.util.decodeBase64(localStorage.getItem("secretKey"));
    const myPublicKey = nacl.util.decodeBase64(localStorage.getItem("publicKey"));
    const recipientKey = nacl.util.decodeBase64(recipientKeyB64);

    const nonce = nacl.randomBytes(nacl.box.nonceLength);
    const encrypted = nacl.box(nacl.util.decodeUTF8(text), nonce, recipientKey, mySecretKey);
    
    const selfNonce = nacl.randomBytes(nacl.box.nonceLength);
    const selfEncrypted = nacl.box(nacl.util.decodeUTF8(text), selfNonce, myPublicKey, mySecretKey);

    safeSend({
        type: "message", 
        to: peerId, 
        from: myId,
        ciphertext: nacl.util.encodeBase64(encrypted),
        nonce: nacl.util.encodeBase64(nonce),
        self_ciphertext: nacl.util.encodeBase64(selfEncrypted),
        self_nonce: nacl.util.encodeBase64(selfNonce),
    });
    pendingSentEl = addMessage(text, "sent", null, "sent");
    lastMessages.set(peerId, {text, kind: "sent" });
    renderFriendList(currentFriends, currentPendingIncoming);
    messageInput.value = "";
});

headerAvatarImg.addEventListener("click", () => {
    chatScreen.hidden = true;
    settingsScreen.hidden = false;
});

logoutBtn.addEventListener("click", () => {
    const token = localStorage.getItem("sessionToken");
    if (token) {
        safeSend({ type: "logout", token });
    }
    localStorage.removeItem("sessionToken");
    chatScreen.hidden = true;
    authScreen.hidden = false;
    myId = null;
    peerId = null;
})

messageInput.addEventListener("input", () => {
    if (!peerId) return;
    const now = Date.now();
    if (now - lastTypingSent > 2000) {
        safeSend({ type: "typing", to: peerId, from: myId });
        lastTypingSent = now;
    }
});


