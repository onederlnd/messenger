#!/usr/bin/env python3
"""
Lightweight GitHub flow helper built on the `gh` CLI.

    python scripts/gitflow.py feature <name>   # branch off main, push
    python scripts/gitflow.py ship [--draft]   # push + open a PR
    python scripts/gitflow.py close            # after merge: cleanup local+remote branch

Assumes `gh` is installed and authenticated, and that your default
branch is named "main" (change DEFAULT_BRANCH below if it's "master").
"""

from __future__ import annotations

import argparse
import subprocess
import sys

DEFAULT_BRANCH = "main"


def run(cmd: list[str], check=True, capture=False) -> subprocess.CompletedProcess:
    print(f"$ {' '.join(cmd)}")
    return subprocess.run(cmd, check=check, text=True, capture_output=capture)


def current_branch() -> str:
    result = run(["git", "rev-parse", "--abbrev-ref", "HEAD"], capture=True)
    return result.stdout.strip()


def cmd_feature(args):
    name = args.name
    branch = f"feature/{name}"
    run(["git", "checkout", DEFAULT_BRANCH])
    run(["git", "pull", "origin", DEFAULT_BRANCH])
    run(["git", "checkout", "-b", branch])
    run(["git", "push", "-u", "origin", branch])
    print(f"\nCreated and pushed '{branch}'. Get to work.")


def cmd_ship(args):
    branch = current_branch()
    if branch == DEFAULT_BRANCH:
        sys.exit(
            f"Refusing to ship from '{DEFAULT_BRANCH}' directly. Checkout a feature branch first."
        )

    run(["git", "push", "-u", "origin", branch])

    pr_cmd = [
        "gh",
        "pr",
        "create",
        "--fill",
        "--base",
        DEFAULT_BRANCH,
        "--head",
        branch,
    ]
    if args.draft:
        pr_cmd.append("--draft")
    run(pr_cmd)
    print(f"\nPR opened for '{branch}'. Once it's approved, run `close` after merging.")


def cmd_close(args):
    branch = current_branch()
    if branch == DEFAULT_BRANCH:
        sys.exit(f"Already on '{DEFAULT_BRANCH}', nothing to close.")

    state = run(
        ["gh", "pr", "view", branch, "--json", "state", "-q", ".state"],
        capture=True,
        check=False,
    ).stdout.strip()

    if state != "MERGED":
        sys.exit(
            f"PR for '{branch}' is not merged yet (state: {state or 'no PR found'}).\n"
            f"Merge it on GitHub (or `gh pr merge {branch} --squash --delete-branch`) before closing."
        )

    run(["git", "checkout", DEFAULT_BRANCH])
    run(["git", "pull", "origin", DEFAULT_BRANCH])
    run(["git", "branch", "-d", branch], check=False)
    run(["git", "push", "origin", "--delete", branch], check=False)
    print(f"\n'{branch}' merged and cleaned up locally and remotely.")


def main():
    parser = argparse.ArgumentParser(
        description="GitHub flow helper (feature/ship/close)"
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_feature = sub.add_parser("feature", help="create a new feature branch")
    p_feature.add_argument("name", help="feature name (branch will be feature/<name>)")
    p_feature.set_defaults(func=cmd_feature)

    p_ship = sub.add_parser("ship", help="push current branch and open a PR")
    p_ship.add_argument("--draft", action="store_true")
    p_ship.set_defaults(func=cmd_ship)

    p_close = sub.add_parser("close", help="clean up after a PR is merged") 
    p_close.set_defaults(func=cmd_close)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
