# Rust phase 2: the CLI

Phase 2 of `plan-rust-migration.md`: the `natamux` command (also `nmx`) in Rust, `crates/natamux-cli`. It has every command of `packages/cli`, the same output and exit codes, and the hook forwarder. This page says how to get it, what CI checks, where it differs, and the gate results.

## Decisions (owner, 2026-10-22)

- Phase 1 is finished first, in the same task (T-0145). Phase 2 then follows.
- No new crates:
  - the options are read by a hand-written port of Node's `util.parseArgs`, with its error texts;
  - HTTP goes through `natamux-client` (hyper);
  - attachments use `base64`, which was already in `Cargo.lock`.
- The gate's E2E specs run in their own job, with the Rust CLI mounted into the daemon container. The images and the other E2E groups stay as they are.
- Phase 2 ships attested CI builds only. The Electron app keeps its Node CLI shim until the cut over.

## Crates

| Crate | Layer | Job in phase 2 |
|---|---|---|
| `natamux-proto` | L0 | The full `Task`, in the order of `taskSchema`. Task events, projects, attachment limits, and the Claude Code hook names. `tests/core_values.rs` fails when these differ from `packages/core` |
| `natamux-client` | L3 | `request`: one HTTP call with headers and a body. It returns the raw status, so the CLI words the daemon's errors itself |
| `natamux-cli` | L4 | The command. `args.rs` reads the options as `parseArgs` does. `command.rs`, `execute.rs`, `daemon.rs`, `format.rs`, `hook.rs`, `hooks.rs` and `run.rs` are ports of the TypeScript files with the same names |

## Get the build

The macOS arm64, Linux x64 (static, musl) and Windows x64 builds are artifacts of every Rust run on a PR into `dev`. They are attested.

```sh
gh run download <run-id> -R lenhatthanh20/natamux -n natamux-cli-macos-arm64
gh attestation verify natamux-cli-macos-arm64.tar.gz -R lenhatthanh20/natamux
tar -xzf natamux-cli-macos-arm64.tar.gz
./natamux help
```

Linux: `natamux-cli-linux-x64.tar.gz`. Windows: `natamux-cli-windows-x64.zip`, with `natamux.exe` and `nmx.exe`.

## What CI checks

| Job | Checks |
|---|---|
| `check` | `cargo xtask ci`, which includes the unit cases ported from `packages/cli/src/__tests__` and the core-values test |
| `cli` | The Linux build. `scripts/cli-parity.sh` runs both CLIs against the daemon image of main and compares stdout, stderr and the exit code: help, usage errors, `hooks claude-code`, read-only commands, and the same steps on one task of each CLI (ids and times made equal first). The Rust CLI alone runs `task create --attach`, `workspace create`, `workspace remove` and a real `hook claude-code`. Then `needs-input`, `shell-agent` and `ask-agent` run with the Rust `natamux` first on the agents' PATH (`compose.rust-cli.yaml`, `NATAMUX_BIN_DIR`). Then the build is attested |
| `app-macos`, `app-windows` | Also build, attest and upload the CLI |

## Where it differs from the TypeScript CLI

- **Plain HTTP only.** There is no TLS without a new crate. A `NATAMUX_URL` that does not start with `http://` gives `Error: NATAMUX_URL must start with http:// (got "…").` with exit 1. `natamux hook` stays silent. The daemon listens on plain HTTP on this machine, as do the task terminals' `NATAMUX_URL`.
- **An answer of the wrong shape** gives `Unexpected answer from the daemon: <what>`, not zod's dump of issues.
- **The `NATAMUX_DEBUG` reasons** of `natamux hook` are this build's, for example `the daemon answered 500` or `The operation was aborted due to timeout`. They are not Node's `fetch failed`.
- **A task state the daemon does not know** prints as `unknown`. zod refused it.
- **Percent-encoding of ids:** only letters, digits and `-._~` stay as they are, so `!*'()` are encoded too. Task ids cannot hold them.

## Gate results

Gate (plan): the CLI unit cases ported as Rust tests. The agent hook E2E specs (`needs-input`, `shell-agent`, `ask-agent`) are green with the Rust CLI on the agents' PATH.

| Part | Date | Result | Notes |
|---|---|---|---|
| Unit cases ported | 2026-10-22 | Pass | 121 tests in `crates/natamux-cli` (`tests/command.rs` 39, `tests/run.rs` 53, `tests/hook.rs` 22, `tests/format.rs` 4, 8 unit tests), plus 4 core-values tests in `natamux-proto`. Every case of `packages/cli/src/__tests__` has a Rust test of the same name, except the 6 "fast hook path" cases: they check how Node loads files. `the_hook_path_never_parses_the_command_line` takes their place. `usage_matches_the_typescript_text` compares `natamux help` with `format.ts` |
| Same output as the TypeScript CLI | 2026-10-22 | Pass | `cli` job of Rust run 39766414434: 73 checks against the daemon image of main, 0 differ. The first run differed only in event numbers, which the daemon counts over all tasks; the script now makes them equal before comparing |
| Agent hook specs with the Rust CLI | 2026-10-22 | Pass | Same run: `needs-input`, `shell-agent` and `ask-agent`, 6 passed, with `/opt/natamux/bin/natamux` first on the agents' PATH |
| Builds and attestations | 2026-10-22 | Pass | `natamux-cli-macos-arm64.tar.gz`, `natamux-cli-linux-x64.tar.gz` and `natamux-cli-windows-x64.zip` each hold `natamux` and `nmx`, and `gh attestation verify` passes for each. The macOS build prints the same `task show` as the Node CLI against the owner's daemon |
