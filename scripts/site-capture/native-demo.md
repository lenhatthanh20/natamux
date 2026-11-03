# Native demo data

Use the published native app, daemon and CLI together. `native-demo.py` has no default command. `prepare` creates a git fixture and starts no app or daemon. It uses the Python standard library and git; it does not run Node, pnpm or cargo.

The repository, tasks, events, worktrees, controls and token live in one new capture folder. The scripts refuse to prepare over an existing folder, seed a daemon that already has tasks, or send actions to a task outside the manifest. Each API call checks the daemon's nonce proof before sending its token. The project has no git remote or Release stage.

The agents are explicitly named **demo-agent** and **demo-reviewer**. They make real files, run real Python tests, call real hooks and use the packaged `natamux done` and `natamux stage back`. Their terminal banner says **Natamux scripted demo agent** and **No model calls**. Use the on-screen caption “Scripted demo agents. Real native workflows.” Do not describe the output as a real provider session or real AI-generated code.

## Start

Run from the repository root in the user's Mac terminal. Keep each quoted path intact.

```sh
capture_root=/private/tmp/natamux-native-capture/take-01
python3 -B scripts/site-capture/native-demo.py prepare \
  --root "$capture_root" \
  --bin-dir /Applications/Natamux.app/Contents/MacOS

# Terminal 1: stays in the foreground. Ctrl-C shuts down its demo terminals too.
python3 -B scripts/site-capture/native-demo.py daemon --root "$capture_root"
```

In another terminal, use the same `capture_root` value:

```sh
capture_root=/private/tmp/natamux-native-capture/take-01
python3 -B scripts/site-capture/native-demo.py seed --root "$capture_root"
python3 -B scripts/site-capture/native-demo.py status --root "$capture_root"
python3 -B scripts/site-capture/native-demo.py app --root "$capture_root" --size 1920x1080
```

Point `--bin-dir` at the downloaded release's `Natamux.app/Contents/MacOS` after mounting and copying the app. The release tested for these scripts is **v1.0.17**. Do not run the daemon with `--help`: that binary starts the daemon rather than parsing a help flag.

`app` connects to this separate daemon with explicit `NATAMUX_URL` and `NATAMUX_TOKEN`. It uses this Mac account's appearance preferences and copies an existing `view.json` into the capture folder before launch. The native app has no preference-folder override. A separate macOS account is needed for full preference isolation. These scripts preserve `HOME` and never replace personal preferences automatically.

`--size` uses logical window dimensions. A 1920 × 1080 window on a 2× display may yield 3840 × 2160 pixels; verify the recorded pixel dimensions with the [native recording guide](native-capture.md). The script does not assert that a screenshot or recording is 4K.

## Seeded tasks

| Manifest name | Prepared state | Recording use |
| --- | --- | --- |
| `gate` | Spec awaits approval | Approve on camera; follow Bug fix → Reproduce → parallel build → review finding → correction → Manual review |
| `review` | Manual review awaits approval | Seven real passing tests, uncommitted checkout diff, editable files and two real task commits |
| `ask` | Spec needs input | Answer the question in the native terminal; observe its state change |
| `running` | Build lanes remain running | Both terminals stay open until explicitly continued; use the Agents grid or task workspace |
| `ready` | Ready, no workspace | Task list and Start a task reference |

`seed` finishes the `review` task through one real send-back. The first implementation clamps the total but misses negative coupons. The reviewer adds a regression that fails, writes its finding, and calls `natamux stage back --to implement`. The second implementation rejects the input. The reviewer runs seven passing tests and leaves the task at Manual review.

That task also has two commits to `demo-review.md`. A separate commit to `catalog-notes.md` on `main` gives the History view a real divergence: two task commits ahead and one main commit behind. The checkout code and tests stay uncommitted at +18/−1.

The isolated project's `.claude/skills/checkout-context/SKILL.md` contains public context about the fictional checkout. In **Start a task**, choose **An agent skill** and select **checkout-context**. The picker reads the real repository-local skill; no provider runs for this capture.

The `running` task pauses both build lanes using a capture control file. Each terminal shows its stage, an actual file excerpt and the real baseline test results. One waiting line updates in place every two seconds.

Seeding uses short pauses. The `gate` task then uses four-second pauses between its actions, so its approval and review loop gives time to follow the native UI. Controls are reread at the start of each stage.

## Actions

Use the native controls while recording. The following commands are also available between takes:

```sh
python3 -B scripts/site-capture/native-demo.py action --root "$capture_root" gate approve
python3 -B scripts/site-capture/native-demo.py action --root "$capture_root" ask answer \
  --text "Reject negative coupons with ValueError."
python3 -B scripts/site-capture/native-demo.py action --root "$capture_root" running continue
python3 -B scripts/site-capture/native-demo.py action --root "$capture_root" gate pace --delay 4
python3 -B scripts/site-capture/native-demo.py status --root "$capture_root"
```

The answer enters the real task terminal through the message API. It does not directly override state. Approving **Manual review** completes the prepared workflow; no push, pull request or publish action occurs.

## Storyboard

Use the same framing, cursor speed and task choices in both themes. Keep the title bar and native controls visible. Capture each shot before changing its state.

| Shot | Native actions | Evidence |
| --- | --- | --- |
| Overview | Open the task list, then Board and Progress | Needs input, approval, running and ready states come from actual daemon events |
| Workflow settings | Open the storefront project's settings; inspect **Bug or feature**, the two paths and the parallel build group | The displayed graph comes from the demo repository's `.natamux.json` |
| Approval and feedback | Open `gate`, read Spec, approve, then open Progress and the two build terminals | Real failing regression, stage reports, automatic send-back and manual approval gate |
| Question | Open `ask`, read the question and type the answer in its terminal | Hook moves to needs-input; prompt hook resumes the real task |
| Diff and editor | Open `review` → Changes; choose `checkout.py`, then `test_checkout.py`; open `checkout.py` from Files in an editor tab | Real git diff and real file contents; the files remain uncommitted |
| History | Open `review` → History | Two note commits on the task branch and a separate catalog note commit on main |
| Task context | Open **Start a task**, choose **An agent skill**, then **checkout-context** | The public fixture skill comes from the isolated project |
| Multiple agents | Open Agents or `running`; show both lanes, then continue between takes | Real native PTYs; banner identifies the scripted demo |

Keep the original 4K recording. Cut and encode only verified native captures. The old browser capture scripts, `--shot` and `--fake-daemon` are separate workflows and are not used here.

No script deletes capture data, worktrees or a prior take. Shut down the capture daemon with Ctrl-C when the take is finished. Retain the private manifest and original recording outside `site/public`; publish only the selected media.
