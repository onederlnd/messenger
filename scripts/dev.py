#!/usr/bin/env python3
"""
Dev environment launcher for messenger.

Boots the pieces you'd otherwise start by hand in separate terminals:
server (cargo run), web frontend (npx serve), and the Android emulator
(with an optional native client build+run on top). Ctrl+C tears
everything down cleanly.

Usage:
    python scripts/dev.py                # server + web + emulator
    python scripts/dev.py --no-emulator  # skip the emulator
    python scripts/dev.py --no-web       # skip the web frontend
    python scripts/dev.py --no-server    # skip the server
    python scripts/dev.py --client       # also build/run the native client (needs emulator)

Configure paths/AVD name in the CONFIG block below or via
scripts/devconfig.toml if present.
"""

from __future__ import annotations

import argparse
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

try:
    import tomllib  # py3.11+
except ImportError:
    tomllib = None

# --------------------------------------------------------------------------
# Config (override via scripts/devconfig.toml)
# --------------------------------------------------------------------------
REPO_ROOT = Path(__file__).resolve().parent.parent

CONFIG = {
    "avd_name": "Pixel_8_Pro",
    "server_dir": "server",
    "server_cmd": ["cargo", "run"],
    "web_dir": "js_frontend",
    "web_cmd": ["npx", "serve", "."],
    "client_dir": "client",
    "client_cmd": ["cargo", "apk", "run", "--target", "x86_64-linux-android", "--lib"],
    "emulator_boot_timeout_s": 120,
    "server_port": 3000,
    "android_package": "com.yourorg.messager",
    "emulator_cold_boot": True,
}

CONFIG_FILE = Path(__file__).resolve().parent / "devconfig.toml"
if CONFIG_FILE.exists() and tomllib is not None:
    with open(CONFIG_FILE, "rb") as f:
        CONFIG.update(tomllib.load(f))

# ANSI colors for log prefixes, cycled per process
COLORS = ["\033[36m", "\033[35m", "\033[33m", "\033[32m", "\033[34m"]
RESET = "\033[0m"


class ManagedProcess:
    """A subprocess whose stdout/stderr is streamed with a colored prefix."""

    def __init__(self, name: str, cmd: list[str], cwd: Path | None, color: str):
        self.name = name
        self.cmd = cmd
        self.cwd = cwd
        self.color = color
        self.proc: subprocess.Popen | None = None
        self._reader_thread: threading.Thread | None = None

    def start(self):
        if shutil.which(self.cmd[0]) is None:
            print(
                f"{self.color}[{self.name}]{RESET} ERROR: '{self.cmd[0]}' not found on PATH"
            )
            return False
        print(
            f"{self.color}[{self.name}]{RESET} starting: {' '.join(self.cmd)} (cwd={self.cwd})"
        )
        self.proc = subprocess.Popen(
            self.cmd,
            cwd=self.cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        self._reader_thread = threading.Thread(target=self._pump_output, daemon=True)
        self._reader_thread.start()
        return True

    def _pump_output(self):
        assert self.proc and self.proc.stdout
        for line in self.proc.stdout:
            print(f"{self.color}[{self.name}]{RESET} {line.rstrip()}")

    def is_alive(self) -> bool:
        return self.proc is not None and self.proc.poll() is None

    def stop(self, timeout=8):
        if not self.proc or self.proc.poll() is not None:
            return
        print(f"{self.color}[{self.name}]{RESET} stopping...")
        self.proc.terminate()
        try:
            self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            print(f"{self.color}[{self.name}]{RESET} did not exit, killing")
            self.proc.kill()


def free_port(port: int):
    """Kill any process already bound to `port` (stale server from a previous run)."""
    result = subprocess.run(["lsof", "-ti", f":{port}"], capture_output=True, text=True)
    pids = [p for p in result.stdout.split() if p]
    for pid in pids:
        print(f"[server] port {port} in use by pid {pid}, killing it")
        subprocess.run(["kill", "-9", pid])


def wait_for_emulator_boot(timeout_s: int) -> bool:
    """Poll adb until the emulator reports sys.boot_completed=1."""
    print("[emulator] waiting for device...")
    try:
        subprocess.run(["adb", "wait-for-device"], timeout=timeout_s, check=False)
    except subprocess.TimeoutExpired:
        return False

    deadline = time.time() + timeout_s
    while time.time() < deadline:
        result = subprocess.run(
            ["adb", "shell", "getprop", "sys.boot_completed"],
            capture_output=True,
            text=True,
        )
        if result.stdout.strip() == "1":
            print("[emulator] boot complete")
            return True
        time.sleep(2)
    return False


def main():
    parser = argparse.ArgumentParser(description="Messenger dev environment launcher")
    parser.add_argument("--no-server", action="store_true")
    parser.add_argument("--no-web", action="store_true")
    parser.add_argument("--no-emulator", action="store_true")
    parser.add_argument(
        "--client", action="store_true", help="also build/run the native client"
    )
    parser.add_argument("--clear-data", action="store_true", help="wipe app data before launching client")
    args = parser.parse_args()

    procs: list[ManagedProcess] = []
    color_i = 0

    def next_color():
        nonlocal color_i
        c = COLORS[color_i % len(COLORS)]
        color_i += 1
        return c

    if not args.no_server:
        free_port(CONFIG["server_port"])
        procs.append(
            ManagedProcess(
                "server",
                CONFIG["server_cmd"],
                REPO_ROOT / CONFIG["server_dir"],
                next_color(),
            )
        )

    if not args.no_web:
        procs.append(
            ManagedProcess(
                "web", CONFIG["web_cmd"], REPO_ROOT / CONFIG["web_dir"], next_color()
            )
        )

    emulator_proc = None
    if not args.no_emulator:
        emulator_cmd = ["emulator", "-avd", CONFIG["avd_name"]]
        if CONFIG.get("emulator_cold_boot"):
            emulator_cmd.append("-no-snapshot-load")
        emulator_proc = ManagedProcess(
            "emulator", emulator_cmd, None, next_color()
        )
        procs.append(emulator_proc)
    

    for p in procs:
        p.start()

    if emulator_proc and emulator_proc.is_alive():
        if not wait_for_emulator_boot(CONFIG["emulator_boot_timeout_s"]):
            print(
                "[emulator] WARNING: boot did not complete within timeout, continuing anyway"
            )

    client_proc = None
    if args.client:
        if args.clear_data:
            print(f"[client] clearing app data for {CONFIG['android_package']}")
            subprocess.run(
                ["adb", "shell", "pm", "clear", CONFIG["android_package"]], check=False
            )
        client_proc = ManagedProcess(
            "client",
            CONFIG["client_cmd"],
            REPO_ROOT / CONFIG["client_dir"],
            next_color(),
        )
        client_proc.start()
        procs.append(client_proc)

    print("\nAll requested processes started. Press Ctrl+C to stop everything.\n")

    def shutdown(*_):
        print("\nShutting down...")
        for p in reversed(procs):
            p.stop()
        sys.exit(0)

    signal.signal(signal.SIGINT, shutdown)
    signal.signal(signal.SIGTERM, shutdown)

    # Idle loop; exit if a critical process (server/web) dies unexpectedly
    try:
        while True:
            time.sleep(2)
            for p in procs:
                if p.name in ("server", "web") and not p.is_alive():
                    print(f"[{p.name}] exited unexpectedly, shutting down the rest")
                    shutdown()
    except KeyboardInterrupt:
        shutdown()


if __name__ == "__main__":
    main()
