#!/usr/bin/env python3
"""Prepare and drive an isolated native Natamux recording. No command runs by default."""

import argparse
import hashlib
import hmac
import json
import os
from pathlib import Path
import secrets
import shlex
import shutil
import socket
import subprocess
import sys
import time
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen


HERE = Path(__file__).resolve().parent
DEFAULT_ROOT = HERE / "out" / "native" / time.strftime("%Y%m%d-%H%M%S")
MARKER = "natamux-native-capture-v1"


def write(path, text, mode=0o600):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as output:
        output.write(text)
    path.chmod(mode)


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    path.chmod(0o600)


def load(root):
    root = root.expanduser().resolve()
    manifest = json.loads((root / "capture.json").read_text())
    if manifest.get("format") != MARKER or manifest.get("root") != str(root):
        raise RuntimeError("This is not a native capture folder")
    return manifest


def clean_env(manifest):
    # Keep the real account identity; no personal shell startup files or agent credentials.
    keep = ("HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "LC_CTYPE")
    env = {name: os.environ[name] for name in keep if name in os.environ}
    root = Path(manifest["root"])
    env.update({
        "PATH": manifest["binDir"] + ":/usr/bin:/bin:/usr/sbin:/sbin",
        "SHELL": str(root / "native-shell.sh"),
        "PS1": "storefront $ ",
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_CONFIG_NOSYSTEM": "1",
        "PYTHONDONTWRITEBYTECODE": "1",
        "NATAMUX_URL": manifest["url"],
        "NATAMUX_TOKEN": (root / "token").read_text().strip(),
        "NATAMUX_HOST": "127.0.0.1",
        "NATAMUX_PORT": str(manifest["port"]),
        "NATAMUX_DATA_DIR": str(root / "data"),
        "NATAMUX_WORKTREES_DIR": str(root / "worktrees"),
        "NATAMUX_REPO_DIR": str(root / "storefront"),
        "NATAMUX_BIN_DIR": manifest["binDir"],
        "NATAMUX_DEMO_ROOT": str(root),
        "NATAMUX_DEMO_PYTHON": manifest["python"],
    })
    return env


def git(repo, env, *args):
    subprocess.run(["/usr/bin/git", "-C", str(repo), *args], env=env, check=True)


def pipeline():
    return {
        "stages": [
            {"id": "context", "kind": "context", "gate": "manual", "output": "context.md"},
            {"id": "spec", "kind": "agent", "gate": "manual", "output": "spec.md",
             "sections": ["Goal", "Scope", "Out of scope", "Acceptance criteria", "Code map"],
             "prompt": "DEMO_STAGE=spec Task: {title}. Write {dir}/spec.md with these sections: Goal, Scope, Out of scope, Acceptance criteria, Code map."},
            {"id": "triage", "name": "Bug or feature", "paths": [
                {"id": "bugfix", "name": "Bug fix",
                 "when": {"type": "output", "file": "context.md", "contains": "Bug:"},
                 "stages": [{"id": "reproduce", "name": "Reproduce the bug", "kind": "agent",
                             "gate": "auto", "output": "repro.md", "sections": ["Steps", "Failing test"],
                             "prompt": "DEMO_STAGE=reproduce Task: {title}. Write {dir}/repro.md."}]},
                {"id": "feature", "name": "Feature", "stages": [
                    {"id": "plan", "kind": "agent", "gate": "manual", "output": "plan.md",
                     "sections": ["Steps", "Files", "Tests"],
                     "prompt": "DEMO_STAGE=plan Task: {title}. Write {dir}/plan.md."}]},
            ]},
            {"id": "build", "name": "Test spec and Implement", "parallel": [
                {"id": "test-spec", "kind": "agent", "gate": "auto", "output": "test-spec.md",
                 "sections": ["Cases"], "ownShell": True, "agent": "demo-reviewer",
                 "prompt": "DEMO_STAGE=test-spec Task: {title}. Write {dir}/test-spec.md."},
                {"id": "implement", "kind": "agent", "gate": "auto", "requireCommit": False,
                 "prompt": "DEMO_STAGE=implement Task: {title}. Fix checkout.py and run the tests. Leave the diff for review."},
            ]},
            {"id": "agent-review", "name": "Review", "kind": "agent", "gate": "auto",
             "output": "review.md", "sections": ["Findings"], "ownShell": True,
             "agent": "demo-reviewer",
             "prompt": "DEMO_STAGE=agent-review Task: {title}. Read the diff. Write {dir}/review.md with a Findings section."},
            {"id": "review", "name": "Manual review", "kind": "review", "gate": "manual"},
        ],
        "tracker": {"type": "none", "createIssue": "never", "linkInPr": False},
    }


CHECKOUT = '''"""Checkout totals for the demo storefront."""

from decimal import Decimal


def subtotal(items):
    return sum((item["price"] * item["quantity"] for item in items), Decimal("0"))


def checkout(items, coupon=Decimal("0")):
    return subtotal(items) - coupon
'''

TESTS = '''from decimal import Decimal
import unittest

from checkout import checkout, subtotal


class CheckoutTests(unittest.TestCase):
    def setUp(self):
        self.items = [{"sku": "mug", "price": Decimal("12"), "quantity": 2}]

    def test_empty_cart(self):
        self.assertEqual(subtotal([]), Decimal("0"))

    def test_cart_total(self):
        self.assertEqual(subtotal(self.items), Decimal("24"))

    def test_coupon_reduces_total(self):
        self.assertEqual(checkout(self.items, Decimal("5")), Decimal("19"))


if __name__ == "__main__":
    unittest.main()
'''


def prepare(args):
    root = args.root.expanduser().resolve()
    bins = args.bin_dir.expanduser().resolve()
    for name in ("natamux-app", "natamux-daemon", "natamux"):
        if not os.access(bins / name, os.X_OK):
            raise RuntimeError("Missing executable: " + str(bins / name))
    if root.exists():
        raise RuntimeError("Choose a new capture folder; existing folders are never overwritten")
    root.mkdir(parents=True, mode=0o700)
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    manifest = {"format": MARKER, "root": str(root), "binDir": str(bins),
                "python": sys.executable, "port": port, "url": f"http://127.0.0.1:{port}",
                "tasks": {}, "release": args.release}
    save(root / "capture.json", manifest)
    write(root / "token", secrets.token_hex(32) + "\n")
    write(root / "native-shell.sh", '#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n', 0o700)
    repo = root / "storefront"
    repo.mkdir()
    write(repo / "README.md", "# Storefront\n\nA fictional shop for native Natamux recordings.\n\nRun the actual checks: `python3 -m unittest -v`.\n")
    write(repo / ".gitignore", "__pycache__/\n.natamux-task/\n")
    write(repo / "checkout.py", CHECKOUT)
    write(repo / "test_checkout.py", TESTS)
    write(repo / "catalog.py", 'PRODUCTS = [\n    {"sku": "mug", "name": "Studio mug", "price": 12},\n    {"sku": "tee", "name": "Cotton T-shirt", "price": 24},\n]\n')
    write(repo / ".natamux.json", json.dumps(pipeline(), indent=2) + "\n")
    env = clean_env(manifest)
    git(repo, env, "init", "-q", "-b", "main")
    git(repo, env, "config", "user.name", "Natamux demo")
    git(repo, env, "config", "user.email", "demo@example.invalid")
    git(repo, env, "add", ".")
    git(repo, env, "commit", "-q", "-m", "Start the demo storefront")
    write(root / "NOTICE.txt", "Fictional project. Scripted demo agents; native UI, terminals, hooks, stage checks, files, tests and git diffs are real.\nNo remote or release stage is configured.\n")
    print(root)


def prove(manifest):
    nonce = secrets.token_hex(16)
    query = urlencode({"nonce": nonce})
    with urlopen(manifest["url"] + "/api/health?" + query, timeout=3) as response:
        health = json.load(response)
    token = (Path(manifest["root"]) / "token").read_text().strip()
    wanted = hashlib.sha256(f"natamux-health-v1\n{token}\n{nonce}".encode()).hexdigest()
    if not hmac.compare_digest(str(health.get("proof", "")), wanted):
        raise RuntimeError("The port does not belong to this isolated capture daemon")


def api(manifest, method, path, body=None):
    prove(manifest)
    token = (Path(manifest["root"]) / "token").read_text().strip()
    request = Request(manifest["url"] + "/api" + path, method=method,
                      data=None if body is None else json.dumps(body).encode(),
                      headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
    try:
        with urlopen(request, timeout=30) as response:
            return json.load(response)
    except HTTPError as error:
        raise RuntimeError(f"{method} {path}: {error.code} {error.read().decode()}") from error


def task(manifest, task_id):
    return api(manifest, "GET", "/tasks/" + task_id)["task"]


def wait_for(manifest, task_id, state, stage, seconds=90):
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        current = task(manifest, task_id)
        if current["state"] == state and current["stage"] == stage:
            return current
        if current["state"] == "failed":
            raise RuntimeError(f"{task_id} failed: {json.dumps(current)}")
        time.sleep(0.5)
    raise RuntimeError(f"{task_id} did not reach {state} {stage}; now {current['state']} {current['stage']}")


def stage(manifest, task_id, action):
    return api(manifest, "POST", f"/tasks/{task_id}/stage", {"action": action})


def seed(manifest):
    root = Path(manifest["root"])
    if manifest["tasks"] or api(manifest, "GET", "/tasks")["tasks"]:
        raise RuntimeError("Seed needs a fresh capture daemon; duplicate tasks are refused")
    repos = api(manifest, "GET", "/repos")["repos"]
    if len(repos) != 1 or Path(repos[0]["path"]).resolve() != root / "storefront":
        raise RuntimeError("The daemon contains a project outside this capture")
    repo_id = repos[0]["id"]
    custom = {"agents": [{"id": name, "base": "other",
                          "command": shlex.join([manifest["python"], str(HERE / "native-agent.py")])}
                         for name in ("demo-agent", "demo-reviewer")]}
    api(manifest, "PUT", "/custom-agents", custom)
    trust = api(manifest, "GET", f"/repos/{repo_id}/trust")
    if not trust["trusted"]:
        api(manifest, "POST", f"/repos/{repo_id}/trust", {"hash": trust["hash"]})
    entries = [
        ("gate", "Fix checkout totals when coupons exceed the cart", "Bug: a coupon larger than the cart must never produce a negative total.", "high", "bug"),
        ("review", "Handle an empty cart at checkout", "Bug: an empty cart with a coupon must have a total of zero.", "normal", "checkout"),
        ("ask", "Decide how checkout handles invalid coupons", "Ask: should a negative coupon be rejected or treated as zero?", "normal", "checkout"),
        ("running", "Check checkout totals across edge cases", "Keep: check checkout and coupon edge cases while the build lanes run side by side.", "high", "tests"),
        ("ready", "Add gift cards at checkout", "Customers can pay part of an order with a gift card.", "normal", "feature"),
    ]
    for key, title, description, priority, label in entries:
        item = api(manifest, "POST", "/tasks", {"title": title, "description": description,
                   "repo": repo_id, "priority": priority, "labels": [label, "demo"], "agent": "demo-agent"})["task"]
        manifest["tasks"][key] = item["id"]
        save(root / "capture.json", manifest)
        write(root / "controls" / (item["id"] + ".json"), json.dumps({"mode": key, "delay": 1.2}))
        if key == "ready":
            continue
        api(manifest, "POST", f"/tasks/{item['id']}/agent", {"agent": "demo-agent"})
        if key == "ask":
            wait_for(manifest, item["id"], "needs-input", "spec")
            continue
        wait_for(manifest, item["id"], "review", "spec")
        if key == "gate":
            continue
        stage(manifest, item["id"], "approve")
        if key == "running":
            wait_for(manifest, item["id"], "review", "plan")
            stage(manifest, item["id"], "approve")
            wait_for(manifest, item["id"], "running", "build")
        else:
            wait_for(manifest, item["id"], "review", "review")
    print(json.dumps(manifest["tasks"], indent=2))


def run(manifest, command, args):
    root = Path(manifest["root"])
    env = clean_env(manifest)
    if command == "daemon":
        # Foreground only: Ctrl-C shuts down the daemon and its demo terminals.
        env.pop("NATAMUX_URL")
        os.chdir(root / "storefront")
        os.execve(str(Path(manifest["binDir"]) / "natamux-daemon"), ["natamux-daemon"], env)
    if command == "app":
        prove(manifest)
        preferences = Path.home() / "Library/Application Support/Natamux/view.json"
        if preferences.exists():
            backup = root / ("view-before-" + time.strftime("%Y%m%d-%H%M%S") + ".json")
            shutil.copy2(preferences, backup)
            backup.chmod(0o600)
        print("App uses this macOS account's appearance preferences; backup is in the capture folder.", flush=True)
        binary = str(Path(manifest["binDir"]) / "natamux-app")
        os.execve(binary, [binary, "--size", args.size], env)
    if command == "seed":
        seed(manifest)
    if command == "status":
        for name, task_id in manifest["tasks"].items():
            current = task(manifest, task_id)
            print(f"{name:8} {task_id} {current['state']:12} {current['stage']:14} {current['title']}")
    if command == "action":
        task_id = manifest["tasks"].get(args.task, args.task)
        if task_id not in manifest["tasks"].values():
            raise RuntimeError("Choose a task recorded in this capture manifest")
        if args.action in ("approve", "skip", "reopen"):
            result = stage(manifest, task_id, args.action)
        elif args.action == "answer":
            result = api(manifest, "POST", f"/tasks/{task_id}/agent/message", {"text": args.text})
        else:
            write(root / "controls" / (task_id + ".continue"), "continue\n")
            result = {"continued": task_id}
        if "task" in result:
            current = result["task"]
            result = {key: current[key] for key in ("id", "state", "stage")}
        print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare", help="create a new local fixture; starts no app or daemon")
    prep.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    prep.add_argument("--bin-dir", type=Path, required=True, help="v1.0.17 .app/Contents/MacOS")
    prep.add_argument("--release", default="v1.0.17")
    for command in ("daemon", "seed", "status", "app", "action"):
        sub = commands.add_parser(command)
        sub.add_argument("--root", type=Path, required=True)
        if command == "app":
            sub.add_argument("--size", default="1920x1080", help="logical points; verify recorded pixels separately")
        if command == "action":
            sub.add_argument("task", help="gate, review, ask, running, ready or a captured task id")
            sub.add_argument("action", choices=("approve", "skip", "reopen", "answer", "continue"))
            sub.add_argument("--text", default="Reject negative coupons with ValueError.")
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args)
    else:
        run(load(args.root), args.command, args)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
