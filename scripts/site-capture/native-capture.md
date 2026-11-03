# Record the native app on the Dell 4K display

The website needs real recordings of the released native app, in dark and light themes. Keep every original `.mov`. Cropping and encoding create separate files. The native recording tools do not launch the app, change tasks, install packages, or write the website's completion manifest.

## Completed capture: 2026-11-03

The completed set uses the released **Natamux 1.0.17** on the Dell P3223QE. Every original recording and screenshot is 3840×2160. The app, files, tests and stage transitions are real; the terminal agents are scripted demo programs.

| Clip | Output pixels | Duration per theme |
| --- | --- | --- |
| `hero` | 1920×1080 | 22 s |
| `status` | 1600×900 | 14 s |
| `progress` | 800×1600 | 22 s |
| `review` | 1600×1000 | 16 s |
| `settings` | 1600×900 | 22 s |
| `editor` | 1600×1000 | 16 s |
| `agents` | 1600×1000 | 16 s |

- Videos and posters: `site/public/media/native/<clip>-<dark|light>.mp4` and `.webp` — seven pairs, fourteen videos and fourteen posters.
- Article images: `site/public/screens/native/` — 46 WebP files. The selected-skill dialog pair is cropped to 800×1127; the other 44 images are 1600×900.
- Social image: `site/public/media/native/og.jpg` — 1200×630, from the actual parallel-build frame.
- Playback index: `site/src/data/native-recordings.json`.
- Source proof: `site/public/media/native/capture-provenance.json` — release asset checksum, raw input hashes, cuts, crop rectangles, output dimensions and hashes. `captureComplete` is true and no screenshots are pending.

The original files and capture evidence are archived at `~/Movies/Natamux/T-0280/2026-11-03/`. The archive includes 65 referenced originals and event sidecars. Draft captures remain separate and are not published.

## Desktop access

The recorder needs access to the logged-in Mac desktop. A restricted shell may report no displays or `could not create image from display`; AppleScript may fail to reach System Events (`-10827`). Run preflight in the desktop terminal when that happens. The full-access capture session can see the Dell P3223QE at display index 3, with 3840×2160 backing pixels.

Run the commands below in Terminal on the Mac desktop with the Dell connected. The kit checks existing permissions; it never opens a permission prompt. If needed, allow that terminal app in **System Settings → Privacy & Security → Screen & System Audio Recording**, then quit and reopen the terminal app. Accessibility permission is only needed for separate GUI input automation. Manual interaction and this recorder do not require it.

## Prepare the shot

1. Verify the latest GitHub release and the installed native app version. Use the release binary; do not build on the owner's Mac.
2. Prepare the isolated demo daemon and repository with [`native-demo.py`](native-demo.md) and launch the app against its explicit `NATAMUX_URL` and `NATAMUX_TOKEN`. Use that guide for the exact arguments.
3. The real app supports `--theme dark|light`, `--size WxH`, and `--open-task TASK`. Window sizes are points, not pixels. Do not pass `--shot` or `--fake-daemon`: `--shot` also selects the fake daemon.
4. The app's macOS preferences still use `~/Library/Application Support/Natamux/view.json`, even with an external daemon. Its data-folder path cannot be overridden through `NATAMUX_DATA_DIR`. Keep existing preferences in mind; do not replace `HOME` for capture.
5. Move the native app onto the Dell and fill the display. Keep private desktop windows outside the recorded display. Use a terminal on the laptop display so starting the recorder does not cover the app.
6. Keep the Dell at 3840×2160 physical pixels. A scaled interface is fine only when the resulting original video is still 3840×2160; the recorder verifies both the display mode and the saved source dimensions.

## Preflight and record

From the repository root, in the Mac desktop terminal:

```sh
bash scripts/site-capture/native-record.sh preflight
```

The JSON lists display names, display indexes, native pixel sizes, point bounds, and existing permissions. Verify which index names the Dell. The following example uses display 3; change it to the listed Dell index.

HiDPI desktop dimensions and capture pixels are different. The preflight reads `pixelWidth` and `pixelHeight` from the active `CGDisplayMode`; `logicalWidth` and `logicalHeight` describe the desktop mode. It does not infer 4K by multiplying the desktop dimensions by the scale. The final video must still pass the independent 3840×2160 check. In the owner's reported display list, **DELL P3223QE is display 3**; display 2 is the portrait U2415. Recheck indexes before each take.

```sh
capture_dir="$PWD/scripts/site-capture/out/native-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$capture_dir"
bash scripts/site-capture/native-record.sh record "$capture_dir/native-dark.mov" --display 3 --seconds 115
```

This records the full physical display with the pointer and click highlights, without audio. After 115 seconds it writes a `.mov.json` sidecar with source dimensions, duration, frame rate, display information, the exact capture command, and the original's SHA-256. It refuses to accept a source smaller or larger than 3840×2160. A failed size check keeps the recording for inspection.

Switch the native app to the light theme and repeat the same interactions:

```sh
bash scripts/site-capture/native-record.sh record "$capture_dir/native-light.mov" --display 3 --seconds 115
```

Use the same framing for both takes. Keep the app's current theme when the recorder starts; change the theme before each recording. The recorder refuses to replace an existing file or sidecar.

## Seven useful clips

These example timings provide room to move between views. Use the actual times from each take when cutting.

| Slot | Example take time | What to show |
| --- | --- | --- |
| `hero` | 3–13 s | Several real terminal panes; switch focus between two running tasks. Keep task states visible in the sidebar. |
| `status` | 17–27 s | Find a question, click Answer, send a reply and inspect the next stage gate. |
| `progress` | 31–43 s | A task's actual stage progress, a gate, and the workflow path or parallel stages. |
| `review` | 47–59 s | Changed files and a readable split diff; select another file or scroll a few lines. |
| `settings` | 63–75 s | Native app settings, followed by the project's workflow editor; select a stage or branch to reveal its settings. |
| `editor` | 79–89 s | Worktree file tree, source file tabs, and a short scroll or selection in the native editor. |
| `agents` | 93–103 s | Three task terminals in the Agents grid; focus a pane and switch between two columns, four columns and Auto. |

Use real task state and real file changes from the isolated demo project. The [native demo guide](native-demo.md) identifies its scripted agents. If terminal output comes from a stand-in agent, retain the disclosure required by the capture kit's [README](README.md). A native recording is not evidence that a stand-in was a real provider run.

## Cut and encode

Copy the example to a new timing file; edit its seconds to match the actual actions in the dark take:

```sh
cp scripts/site-capture/native-record.cuts.example.json "$capture_dir/cuts-dark.json"
```

Every `crop` is `[x, y, width, height]` in pixels of the original 3840×2160 recording. `[0, 0, 3840, 2160]` keeps the whole Dell. Use an even, in-bounds rectangle to remove desktop margins when the app did not fill the display. Do not stretch or upscale a smaller source. `width: 1920` yields a 1920×1080 clip from the full 4K source; other crops keep their own aspect ratio.

For a montage, replace `start` and `end` with chronological ranges such as `"ranges": [[2, 5], [14, 20], [30, 34]]`. Each range is a cut from the real recording; the output joins them at 30 fps. `posterAt` can name an original timestamp inside one of the ranges. Without it, the poster uses the first frame. The ranges and source SHA-256 remain in the provenance report.

```sh
bash scripts/site-capture/native-record.sh encode "$capture_dir/native-dark.mov" "$capture_dir/cuts-dark.json" "$capture_dir/encoded-dark"
```

Create `cuts-light.json` from the same example, rename each entry from `-dark` to `-light`, and set its times from the light take:

```sh
bash scripts/site-capture/native-record.sh encode "$capture_dir/native-light.mov" "$capture_dir/cuts-light.json" "$capture_dir/encoded-light"
```

Encoding uses an existing `ffmpeg`, an explicit binary set through `NATAMUX_CAPTURE_FFMPEG`, or the existing Docker `linuxserver/ffmpeg:latest` image through `ffm.sh`. It does not pull an image or install software. Each output is H.264 High, 30 fps, yuv420p, CRF 22, no audio, with fast start. A variable-frame-rate screen recording can hold its last real sample through the declared cut endpoint. Each clip has a WebP poster. Each output folder has `capture-provenance.json` linking its cuts and encoded dimensions to the original's SHA-256. Existing output files cause a stop.

Inspect any original or clip:

```sh
bash scripts/site-capture/native-record.sh inspect "$capture_dir/native-dark.mov"
bash scripts/site-capture/native-record.sh inspect "$capture_dir/encoded-dark/hero-dark.mp4"
```

Review all fourteen clips for framing, readable text, smooth interactions, and private information. Keep the original full-display recordings and sidecars outside the website. Copy only the reviewed `.mp4` and `.webp` outputs into `site/public/media/native/`. Their names are `hero`, `status`, `progress`, `review`, `settings`, `editor`, and `agents`, each with `-dark` and `-light` suffixes.

Update `site/src/data/native-recordings.json` only after both themes are verified, using the actual installed app version, capture date, and encoded width/height. Leave its incomplete state intact until those recordings exist. Old screenshots and old browser clips do not fulfill this native capture requirement.

## Publish provenance and archive originals

`native-record.provenance.py` verifies published videos, posters and screenshots against their retained cut reports and original hashes. Set `NATAMUX_CAPTURE_FFPROBE` to an existing `ffprobe` when it is not on `PATH`. Its command-line options are available with:

```sh
python3 -B scripts/site-capture/native-record.provenance.py --help
```

The helper requires the six homepage pairs (`hero`, `status`, `progress`, `review`, `settings`, `editor`). It also discovers additional paired recordings, including the completed `agents` pair. It checks matching theme dimensions and frame counts, records the release asset checksum and uses filenames instead of private source paths. `--refresh` can add verified captures to the same release's report; it refuses to change earlier entries or clear a pending screenshot without its file.

Before closing a capture session:

1. Collect the unique `raw.filename` and `raw.sha256` values from the public provenance's recordings, screenshots and social image.
2. Copy those original `.mov` and `.png` files into a durable archive. Include each recording's event and stage sidecars. The completed 2026-11-03 archive contains 51 original captures and 14 event/stage sidecars.
3. Copy the cut manifests, private processing reports and final public provenance into the archive. Keep demo credentials and daemon configuration outside the public website.
4. Verify the copied originals against the recorded SHA-256 values. Keep the sources and drafts; archiving does not remove anything.

For this capture, the archive is `~/Movies/Natamux/T-0280/2026-11-03/`. Future captures should use a new dated folder. Do not replace the original full-display files with cropped outputs.

## Historical docs reshoot mapping

Before the native refresh, the articles referenced **26 distinct legacy sources: 22 still states and four clips**. This table preserves that migration map. Production articles now use the native sources; the old paths below are historical and must not be restored to article markup. Existing legacy files remain on disk until their removal is authorized.

Native still paths below start with `/screens/native/`; video paths start with `/media/native/`. Each stem has both theme suffixes.

| Historical source stem | Completed native stem | Captured state |
| --- | --- | --- |
| `/screens/v2/agents` | `/screens/native/agents` | Three scripted task terminals in the Agents grid. |
| `/screens/v2/board-wide` | `/screens/native/board-wide` | Board layout with the sidebar visible. |
| `/screens/v2/changes-generate` | `/screens/native/changes-generate` | Split diff and empty commit message controls. |
| `/screens/v2/docs` | `/screens/native/docs` | Rendered `spec.md` and the stage file list. |
| `/screens/v2/history` | `/screens/native/history` | Actual commit graph: two ahead of main, one behind. |
| `/screens/v2/palette` | `/screens/native/palette` | Open palette with tasks and suggested commands. |
| `/screens/v2/progress` | `/screens/native/progress` | Project progress layout across the configured stages. |
| `/screens/v2/progress-not-taken` | `/screens/native/progress-not-taken` | Expanded untaken Bug fix path; Feature was taken. |
| `/screens/v2/progress-took-develop` | `/screens/native/progress-took-feature` | Feature taken through Otherwise. |
| `/screens/v2/settings-review-agent` | `/screens/native/settings-review-agent` | Review settings with `demo-reviewer` selected. |
| `/screens/v2/settings-stages` | `/screens/native/settings-stages` | Workflow editor with Spec selected. |
| `/screens/v2/stage-agent-picker` | `/screens/native/stage-agent-picker` | Open agent dropdown with configured agent options. |
| `/screens/v2/stages-branch` | `/screens/native/stages-branch` | If branch selected with its condition and paths. |
| `/screens/v2/start-task` | `/screens/native/start-task` | Filled notes dialog with title, attachments and agent fields. |
| `/screens/v2/start-task-skill` | `/screens/native/start-task-skill` | Cropped dialog: `checkout-context`, reference `checkout.py`. |
| `/screens/v2/table` | `/screens/native/table` | Native tasks table and sidebar. |
| `/screens/v2/task-details` | `/screens/native/task-details` | Notes, metadata and activity beside three terminals. |
| `/screens/v2/task-needs-you` | `/screens/native/task-needs-you` | Pending negative-coupon question and Answer action. |
| `/screens/v2/task-review-shell5` | `/screens/native/task-review-shell5` | Three terminals including the running Review stage. |
| `/screens/v2/task-sent-back` | `/screens/native/task-sent-back` | Review ×2 after returning work, at the final manual gate. |
| `/screens/v2/task-side-by-side` | `/screens/native/task-side-by-side` | Test spec and Implement running side by side. |
| `/screens/v2/task-spec-approve` | `/screens/native/task-spec-approve` | Spec waiting at its approval gate. |
| `/media/v2/clip-agents` | `/media/native/agents` | Three task panes; focus and layout changes. |
| `/media/v2/clip-branch` | `/media/native/settings` | Select workflow stages and a branch, then inspect JSON. |
| `/media/v2/clip-needs-you` | `/media/native/status` | Answer a real terminal question and return to the Spec gate. |
| `/media/v2/clip-send-back` | `/media/native/progress` | Actual cuts through approval, parallel work, review and retry. |

For future reshoots, record full-Dell takes for these states. Export a still from the exact frame where the native UI is ready:

```sh
bash scripts/site-capture/native-record.sh still "$capture_dir/docs-dark.mov" "$capture_dir/docs-stills/start-task-dark.webp" --at 12.5 --width 1600
```

`still` also accepts `--crop X Y W H` using original pixels. It verifies the 4K video source and keeps a separate provenance sidecar. Put reviewed article stills in `site/public/screens/native/` and article videos in `site/public/media/native/`, each with both theme suffixes. Do not change article paths until the matching files exist. The article screenshots need no fake-daemon mode; prepare the corresponding real demo state instead.

For a full-Dell 3840×2160 PNG screenshot, use the image path. It uses the existing `cwebp` encoder and retains a separate provenance sidecar:

```sh
bash scripts/site-capture/native-record.sh image "$capture_dir/start-task-dark.png" "$capture_dir/docs-stills/start-task-dark.webp" --crop 0 0 3840 2160 --width 1600
```
