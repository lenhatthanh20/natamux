#!/usr/bin/env python3
"""What a change needs from the Rust CI (.github/workflows/rust.yml), and cache-friendly mtimes.

Usage: ci-changes.py flags | crates | mtimes | stamp

flags   writes key=value lines to $GITHUB_OUTPUT (stdout when unset): the job flags, the changed
        crates with their dependents, the E2E groups to run (`[e2e: a,b]`; the flag `e2e`
        says whether the Rust daemon needs E2E at all), `all` and the shot shards (`Shots:`
        line, `[ci: shots]` or `[ci: all]`; none on main) and `shots`, the states of the
        `Shots:` line.
        A head commit message with `[ci: all]` runs everything. A push to main reads the push
        event's before commit; a pull request and snapshot compare against origin/main.
crates  prints the same crate list, one per line.
mtimes  sets file mtimes so cargo rebuilds only what changed since the cache was saved.
stamp   records HEAD in target/.ci-commit; run it before the cache is saved.
"""

import fnmatch
import json
import os
import pathlib
import re
import subprocess
import sys
import tomllib

BASE = "origin/main"
E2E_GROUPS = ["timing", "cache", "terminal", "rest"]
DAEMON_CRATES = [
    "natamux-core",
    "natamux-store",
    "natamux-pty",
    "natamux-screen",
    "natamux-agents",
    "natamux-git",
    "natamux-stages",
    "natamux-daemon",
]
STAMP = pathlib.Path("target/.ci-commit")
OLD_TIME = 946684800  # 2000-01-01

DAEMON_PATHS = [
    *(f"crates/natamux-{name}/" for name in ("proto", "core", "store", "pty", "screen", "agents", "stages", "daemon")),
    "crates/natamux-git/",
    "scripts/fake-agent.sh",
    "scripts/fake-gh.sh",
]
TERMINAL_PATHS = [
    "crates/natamux-pty/",
    "crates/natamux-screen/",
    "crates/natamux-daemon/src/terminals/",
    "crates/natamux-daemon/tests/terminal_bench.rs",
]
APP_PATHS = [
    "crates/natamux-app/",
    "crates/natamux-ui/",
    "crates/natamux-update/",
    "crates/natamux-daemon/src/main.rs",
    "crates/natamux-daemon/src/config.rs",
    "crates/natamux-daemon/src/start.rs",
    "scripts/*shots*",
    "scripts/bundle-rust-app.sh",
    "scripts/motion-cost.sh",
]
# Runtime inputs also affect the live client tests. Build inputs affect the images and E2E.
IMAGE_RUNTIME_PATHS = ["scripts/e2e.sh", "compose.e2e.yaml", "compose.prod.yaml", "docker/prod.Dockerfile", "e2e/"]
IMAGE_PATHS = [
    *IMAGE_RUNTIME_PATHS,
    "packages/core/", "packages/web/", "package.json", "pnpm-lock.yaml",
    "pnpm-workspace.yaml", "tsconfig.base.json", "docker/nginx.conf",
]
# E2E runs the daemon binary in the images of main. The images also hold the CLI, whose agent
# hook specs run in the rest group.
E2E_PATHS = [
    *DAEMON_PATHS,
    *IMAGE_PATHS,
    "crates/natamux-cli/",
    "crates/natamux-client/",
]
CLIENT_PATHS = [
    *(f"crates/natamux-{name}/" for name in ("client", "cli", "proto")),
    *IMAGE_RUNTIME_PATHS,
]
LOCK_PATHS = ["Cargo.lock", "deny.toml", "supply-chain/", ".cargo/audit.toml"]
CHECK_PATHS = [
    "crates/", "xtask/", "scripts/", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
    ".cargo/", "deny.toml", "supply-chain/", ".github/workflows/rust.yml",
    ".github/workflows/release.yml", "docs/releases/", "clippy.toml", "package.json",
    "packages/web/package.json", "packages/core/package.json", "site/src/pages/index.astro",
    "site/src/content/docs/docs/",
]
WEB_PATHS = [
    "packages/core/", "packages/web/", "package.json", "pnpm-lock.yaml",
    "pnpm-workspace.yaml", "tsconfig.base.json", "biome.json", ".github/workflows/ci.yml",
]
RELEASE_NOTE = "docs/releases/v*.md"
# Cargo.lock: a new version of a dependency can break any build.
ALL_PATHS = [
    "rust-toolchain.toml",
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/",
    "xtask/",
    "scripts/ci-wait-artifact.sh",
]
MAIN_FULL_PATHS = [".github/workflows/rust.yml", "scripts/ci-changes.py"]
NO_CI_PATHS = ["docs/", "site/", ".claude/", "README.md"]
ROUTED_PATHS = [
    *DAEMON_PATHS, *TERMINAL_PATHS, *APP_PATHS, *IMAGE_PATHS, *E2E_PATHS,
    *CLIENT_PATHS, *LOCK_PATHS, *CHECK_PATHS, *WEB_PATHS, *ALL_PATHS, *MAIN_FULL_PATHS,
    *NO_CI_PATHS,
]
SHARDS = [1, 2, 3, 4]


def matches(path, patterns):
    """A pattern ending in / is a folder prefix; any other is a glob."""
    return any(
        path.startswith(p) if p.endswith("/") else fnmatch.fnmatchcase(path, p) for p in patterns
    )


def full_run(files, ref=""):
    """Run all jobs for missing or unknown inputs; defer CI config coverage to main for PRs."""
    full_paths = ALL_PATHS if ref.startswith("refs/pull/") else [*ALL_PATHS, *MAIN_FULL_PATHS]
    return files is None or any(
        matches(path, full_paths) or not matches(path, ROUTED_PATHS) for path in files
    )


def path_flags(files, ref=""):
    """The job flags for a list of changed paths; None means the diff failed."""
    if full_run(files, ref):
        return dict.fromkeys(
            ["daemon", "terminal", "app", "client", "lock", "e2e", "check", "web", "release_prep"],
            True,
        )
    daemon = any(matches(f, DAEMON_PATHS) for f in files)
    return {
        "daemon": daemon,
        "terminal": any(matches(f, TERMINAL_PATHS) for f in files),
        "app": any(matches(f, APP_PATHS) for f in files),
        "client": any(matches(f, CLIENT_PATHS) for f in files),
        "lock": any(matches(f, LOCK_PATHS) for f in files),
        "e2e": any(matches(f, E2E_PATHS) for f in files),
        "check": any(matches(f, CHECK_PATHS) for f in files),
        "web": any(matches(f, WEB_PATHS) for f in files),
        "release_prep": any(matches(f, [RELEASE_NOTE]) for f in files),
    }


def workspace_graph(root="."):
    """{crate: set of workspace crates it depends on}, from the Cargo.toml files."""
    root = pathlib.Path(root)
    graph = {}
    for manifest in [*sorted(root.glob("crates/*/Cargo.toml")), root / "xtask" / "Cargo.toml"]:
        if not manifest.is_file():
            continue
        data = tomllib.loads(manifest.read_text())
        tables = [data, *data.get("target", {}).values()]
        deps = set()
        for table in tables:
            for kind in ("dependencies", "dev-dependencies", "build-dependencies"):
                for name, spec in table.get(kind, {}).items():
                    local = isinstance(spec, dict) and ("path" in spec or spec.get("workspace"))
                    # `ui = { package = "natamux-ui", ... }` names the crate under another key.
                    crate = spec.get("package", name) if isinstance(spec, dict) else name
                    if crate.startswith("natamux-") and local:
                        deps.add(crate)
        graph[data["package"]["name"]] = deps
    return graph


def changed_crates(files, graph):
    """Crates with a changed file, plus every crate that depends on one, transitively."""
    dirs = {name: name for name in graph}
    out = set()
    for f in files:
        parts = f.split("/")
        if parts[0] == "xtask":
            out.add("xtask")
        elif parts[0] == "crates" and len(parts) > 2 and parts[1] in dirs:
            out.add(parts[1])
    grew = True
    while grew:
        grew = False
        for name, deps in graph.items():
            if name not in out and deps & out:
                out.add(name)
                grew = True
    return sorted(out & set(graph))


def parse_e2e_groups(message):
    """The groups named by an `[e2e: a,b]` tag; all when there is no tag. Raises ValueError."""
    found = re.search(r"\[e2e:([^\]]*)\]", message or "")
    if not found:
        return list(E2E_GROUPS)
    names = [n.strip() for n in found.group(1).split(",")]
    unknown = [n for n in names if n not in E2E_GROUPS]
    if unknown:
        raise ValueError(f"unknown e2e group {unknown}; valid names: {', '.join(E2E_GROUPS)}")
    return [g for g in E2E_GROUPS if g in names]


def parse_all(message):
    """True when the message has the tag `[ci: all]`."""
    return re.search(r"\[ci:\s*all\s*\]", message or "") is not None


def wants_all(ref, message):
    """The head commit message can request every job on a push or pull request."""
    return parse_all(message)


def shot_shards(message, ref):
    """The app-shots jobs, on demand only: all of SHARDS for `[ci: shots]` or `[ci: all]`, one for
    a `Shots:` line that picks a few states, else none. main never shoots: with REPORT_ONLY nothing
    compares the shots, and the 4 macOS jobs waited up to an hour for a runner."""
    if ref == "refs/heads/main":
        return []
    if parse_all(message) or re.search(r"\[ci:\s*shots\s*\]", message or ""):
        return list(SHARDS)
    if re.search(r"^Shots:", message or "", re.MULTILINE):
        return [1]
    return []


def compute(files, message, run_all, graph, ref=""):
    """All outputs of `flags`. run_all: from wants_all."""
    if run_all:
        files = None
    flags = path_flags(files, ref)
    everything = full_run(files, ref)
    if everything:
        crates = sorted(graph)
    else:
        crates = changed_crates(files, graph)
    groups = list(E2E_GROUPS) if run_all or ref == "refs/heads/main" else parse_e2e_groups(message)
    daemon = [c for c in DAEMON_CRATES if c in crates]
    out = {k: str(v).lower() for k, v in flags.items()}
    out["crates"] = " ".join(crates)
    out["daemon_crates"] = " ".join(f"-p {c}" for c in daemon)
    out["e2e_groups"] = json.dumps(groups)
    out["all"] = str(run_all).lower()
    out["shards"] = str(shot_shards(message, ref))
    out["shots"] = shot_states(message, ref)
    return out


def shot_states(message, ref):
    """The states of the first `Shots:` line, for the app-shots jobs; "" for every state. main
    never picks states."""
    found = re.search(r"^Shots: *(.*)$", message or "", re.MULTILINE)
    return found.group(1).strip() if found and ref != "refs/heads/main" else ""


def parse_base(message):
    """The commit a `[base: <sha>]` tag names, else origin/main. For measuring CI on a snapshot as
    if its other changes were on main already."""
    found = re.search(r"\[base:\s*([0-9a-f]{7,40})\s*\]", message or "")
    return found.group(1) if found else BASE


def git(*args, check=True):
    return subprocess.run(["git", *args], capture_output=True, text=True, check=check)


def changed_files(base, three_dot=True):
    try:
        dots = "..." if three_dot else ".."
        return git("diff", "--name-only", f"{base}{dots}HEAD").stdout.split()
    except (subprocess.CalledProcessError, OSError):
        return None


def push_base():
    """The commit before this checkout's push; None makes an unknown range run all jobs."""
    try:
        event = json.loads(pathlib.Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    except (KeyError, OSError, ValueError):
        return None
    if not isinstance(event, dict):
        return None
    before = event.get("before")
    if not isinstance(before, str) or not re.fullmatch(r"[0-9a-f]{40}", before) or before == "0" * 40:
        return None
    after = event.get("after")
    if not isinstance(after, str) or not re.fullmatch(r"[0-9a-f]{40}", after):
        return None
    try:
        if git("rev-parse", "HEAD").stdout.strip() != after:
            return None
    except (subprocess.CalledProcessError, OSError):
        return None
    return before


def head_message():
    return os.environ.get("HEAD_MESSAGE") or git("log", "-1", "--format=%B").stdout


def pr_head_message():
    """The message of a pull request's head commit: the second parent of the merge checkout."""
    return git("log", "-1", "--format=%B", "HEAD^2", check=False).stdout


def outputs():
    ref = os.environ.get("GITHUB_REF", "")
    event = os.environ.get("GITHUB_EVENT_NAME", "")
    # The head of a pull request is a merge commit: only [ci: all] is read, from the PR's own head.
    pull_request = event == "pull_request"
    message = "" if pull_request else head_message()
    run_all = wants_all(ref, pr_head_message() if pull_request else message)
    if run_all:
        files = None
    elif event == "push" and ref == "refs/heads/main":
        base = push_base()
        files = changed_files(base, three_dot=False) if base else None
    else:
        files = changed_files(parse_base(message))
    out = compute(files, message, run_all, workspace_graph(), ref)
    if pull_request:
        # As [ci: all], a PR reads its shot tags from its own head commit.
        head = pr_head_message()
        out["shards"] = str(shot_shards(head, ref))
        out["shots"] = shot_states(head, ref)
    return out


def known_commit(root):
    """The commit in target/.ci-commit when git has it, else None."""
    stamp = root / STAMP
    if not stamp.is_file():
        return None
    sha = stamp.read_text().strip()
    exists = ["-C", str(root), "cat-file", "-e", sha + "^{commit}"]
    if git(*exists, check=False).returncode != 0:
        # A shallow checkout lacks it: fetch just that commit.
        git("-C", str(root), "fetch", "-q", "--depth=1", "origin", sha, check=False)
        if git(*exists, check=False).returncode != 0:
            return None
    return sha


def set_mtimes(root="."):
    # Cargo decides freshness of workspace crates by mtime, and a fresh checkout makes every file new.
    root = pathlib.Path(root)
    tracked = git("-C", str(root), "ls-files").stdout.splitlines()
    # Windows cannot set the time of a link itself.
    follow = os.utime not in os.supports_follow_symlinks
    for f in tracked:
        try:
            os.utime(root / f, (OLD_TIME, OLD_TIME), follow_symlinks=follow)
        except OSError:
            pass
    sha = known_commit(root)
    touch = git("-C", str(root), "diff", "--name-only", sha, "HEAD").stdout.split() if sha else tracked
    for f in touch:
        if (root / f).exists():
            os.utime(root / f, None)


def stamp_head(root="."):
    path = pathlib.Path(root) / STAMP
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(git("-C", str(root), "rev-parse", "HEAD").stdout.strip() + "\n")


def main(argv):
    command = argv[0] if argv else ""
    if command == "flags":
        try:
            out = outputs()
        except ValueError as error:
            print(f"ci-changes: {error}", file=sys.stderr)
            return 1
        text = "".join(f"{k}={v}\n" for k, v in out.items())
        target = os.environ.get("GITHUB_OUTPUT")
        if target:
            with open(target, "a") as handle:
                handle.write(text)
        print(text, end="")
    elif command == "crates":
        print(*outputs()["crates"].split(), sep="\n")
    elif command == "mtimes":
        set_mtimes()
    elif command == "stamp":
        stamp_head()
    else:
        print(__doc__, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
