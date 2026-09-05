# Messenger — Command Reference

## Server

```bash
cd server
cargo run                    # start the server
cargo build                  # build without running
```

Server reads `.env` for `TLS_CERT_PATH`, `TLS_KEY_PATH`, `BIND_IP`, `BIND_PORT` — defaults to `0.0.0.0:3000` if unset. Creates `messages.db` on first run in the working directory.

## Web frontend (js_frontend)

```bash
cd web_client
npx serve .                  # serve static files
```

## Native client (Dioxus)

```bash
cd client
dx serve --platform desktop  # run on desktop, hot-reload where possible
dx serve --platform android  # run on Android (needs emulator running)
dx serve --platform ios      # run on iOS (macOS + Xcode only)
```

```bash
# start an Android emulator manually if not already running
~/Android/Sdk/emulator/emulator -list-avds        # list available emulators
~/Android/Sdk/emulator/emulator -avd <name>         # boot one
```

## Build/check without running

```bash
cargo build                        # from workspace root — builds all members
cargo build -p messenger-server    # build just the server crate
cargo build -p client              # build just the client crate
cargo check                        # faster — type-check without producing a binary
```

## Killing stuck processes

```bash
lsof -i :3000                # find what's using the server port
kill $(lsof -t -i :3000)     # kill it
kill -9 $(lsof -t -i :3000)  # force kill if it won't die
```

## Env vars this project needs set (once, in `~/.bashrc`)

```bash
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_SDK_ROOT="$HOME/Android/Sdk"
export ANDROID_NDK_HOME="$HOME/Android/Sdk/ndk/<version>"
export JAVA_HOME="/snap/android-studio/current/jbr"
```

```bash
source ~/.bashrc             # reload after editing
echo $ANDROID_NDK_HOME        # verify a var is set
```

## Misc

```bash
cargo install dioxus-cli --locked   # (re)install the dx CLI
dx new <name>                        # scaffold a new Dioxus project
git status                            # check what's changed before committing
```gitflow test Sat Sep  5 01:07:04 PM EDT 2026
