# Record the native app on the Dell 4K display

The website needs real recordings of the released native app, in dark and light themes. Keep every original `.mov`. Cropping and encoding create separate files. The native recording tools do not launch the app, change tasks, install packages, or write the website's completion manifest.

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
bash scripts/site-capture/native-record.sh record "$capture_dir/native-dark.mov" --display 3 --seconds 100
```

This records the full physical display with the pointer and click highlights, without audio. After 100 seconds it writes a `.mov.json` sidecar with source dimensions, duration, frame rate, display information, the exact capture command, and the original's SHA-256. It refuses to accept a source smaller or larger than 3840×2160. A failed size check keeps the recording for inspection.

Switch the native app to the light theme and repeat the same interactions:

```sh
bash scripts/site-capture/native-record.sh record "$capture_dir/native-light.mov" --display 3 --seconds 100
```

Use the same framing for both takes. Keep the app's current theme when the recorder starts; change the theme before each recording. The recorder refuses to replace an existing file or sidecar.

## Six useful clips

These example timings provide room to move between views. Use the actual times from each take when cutting.

| Slot | Example take time | What to show |
| --- | --- | --- |
| `hero` | 3–13 s | Several real terminal panes; switch focus between two running tasks. Keep task states visible in the sidebar. |
| `status` | 17–27 s | Task list with several states; select a task, then switch to the board. |
| `progress` | 31–43 s | A task's actual stage progress, a gate, and the workflow path or parallel stages. |
| `review` | 47–59 s | Changed files and a readable split diff; select another file or scroll a few lines. |
| `settings` | 63–75 s | Native app settings, followed by the project's workflow editor; select a stage or branch to reveal its settings. |
| `editor` | 79–89 s | Worktree file tree, source file tabs, and a short scroll or selection in the native editor. |

Use real task state and real file changes from the isolated demo project. The [native demo guide](native-demo.md) identifies its scripted agents. If terminal output comes from a stand-in agent, retain the disclosure required by the capture kit's [README](README.md). A native recording is not evidence that a stand-in was a real provider run.

## Cut and encode

Copy the example to a new timing file; edit its seconds to match the actual actions in the dark take:

```sh
cp scripts/site-capture/native-record.cuts.example.json "$capture_dir/cuts-dark.json"
```

Every `crop` is `[x, y, width, height]` in pixels of the original 3840×2160 recording. `[0, 0, 3840, 2160]` keeps the whole Dell. Use an even, in-bounds rectangle to remove desktop margins when the app did not fill the display. Do not stretch or upscale a smaller source. `width: 1920` yields a 1920×1080 clip from the full 4K source; other crops keep their own aspect ratio.

```sh
bash scripts/site-capture/native-record.sh encode "$capture_dir/native-dark.mov" "$capture_dir/cuts-dark.json" "$capture_dir/encoded-dark"
```

Create `cuts-light.json` from the same example, rename each entry from `-dark` to `-light`, and set its times from the light take:

```sh
bash scripts/site-capture/native-record.sh encode "$capture_dir/native-light.mov" "$capture_dir/cuts-light.json" "$capture_dir/encoded-light"
```

Encoding uses an existing `ffmpeg`, an explicit binary set through `NATAMUX_CAPTURE_FFMPEG`, or the existing Docker `linuxserver/ffmpeg:latest` image through `ffm.sh`. It does not pull an image or install software. Each output is H.264 High, 30 fps, yuv420p, CRF 22, no audio, with fast start. Each clip has a WebP poster from its first frame. Each output folder has `capture-provenance.json` linking its cuts and encoded dimensions to the original's SHA-256. Existing output files cause a stop.

Inspect any original or clip:

```sh
bash scripts/site-capture/native-record.sh inspect "$capture_dir/native-dark.mov"
bash scripts/site-capture/native-record.sh inspect "$capture_dir/encoded-dark/hero-dark.mp4"
```

Review all twelve clips for framing, readable text, smooth interactions, and private information. Keep the original full-display recordings and sidecars outside the website. Copy only the reviewed `.mp4` and `.webp` outputs into `site/public/media/native/`. Their names are `hero`, `status`, `progress`, `review`, `settings`, and `editor`, each with `-dark` and `-light` suffixes.

Update `site/src/data/native-recordings.json` only after both themes are verified, using the actual installed app version, capture date, and encoded width/height. Leave its incomplete state intact until those recordings exist. Old screenshots and old browser clips do not fulfill this native capture requirement.

## Docs article reshoots

The six landing clips do not refresh the articles. An inventory of `<Figure src=...>` in the current docs finds **26 distinct legacy sources: 22 still states and four clips**, many used on several pages. Reshoot each state in both themes, then update the corresponding figure path and alt text to match the actual native app. Keep the old assets until all their references are replaced and removal is authorized.

| Current source stem | Native state or action to reshoot |
| --- | --- |
| `/screens/v2/agents` | Agents grid, including one task waiting for input. |
| `/screens/v2/board-wide` | Board layout with a useful mix of task states. |
| `/screens/v2/changes-generate` | Changed files, split diff, and the commit message controls. |
| `/screens/v2/docs` | Stage files list and a rendered document. |
| `/screens/v2/history` | Real commits on a task branch and its base. |
| `/screens/v2/palette` | Command palette with a search or command list. |
| `/screens/v2/progress` | Project progress layout, with several stages and task states. |
| `/screens/v2/progress-not-taken` | The expanded path that did not run. |
| `/screens/v2/progress-took-develop` | The branch path that ran and the reason it was chosen. |
| `/screens/v2/settings-review-agent` | Review stage settings with a different agent selected. |
| `/screens/v2/settings-stages` | Project workflow editor and stage details. |
| `/screens/v2/stage-agent-picker` | Stage Agent and Arguments fields. |
| `/screens/v2/stages-branch` | Branch condition with both workflow paths visible. |
| `/screens/v2/start-task` | Start task dialog with notes, workspace, and agent fields. |
| `/screens/v2/start-task-skill` | Start task dialog using an available agent skill. |
| `/screens/v2/table` | Native tasks table and sidebar. |
| `/screens/v2/task-details` | Task details, workspace, subtasks, and activity. |
| `/screens/v2/task-needs-you` | A real pending question and the available actions. |
| `/screens/v2/task-review-shell5` | A review stage running in its own shell. |
| `/screens/v2/task-sent-back` | Review findings and the next Implement attempt. |
| `/screens/v2/task-side-by-side` | Test spec and Implement running side by side. |
| `/screens/v2/task-spec-approve` | A stage waiting at its approval gate. |
| `/media/v2/clip-agents` | Switch focus between active panes in the Agents grid. |
| `/media/v2/clip-branch` | Select a branch and inspect its condition and paths. |
| `/media/v2/clip-needs-you` | A question arrives, is answered, and the task continues. |
| `/media/v2/clip-send-back` | Review sends findings back and Implement runs again. |

Record additional full-Dell takes for these states. Export a still from the exact frame where the native UI is ready:

```sh
bash scripts/site-capture/native-record.sh still "$capture_dir/docs-dark.mov" "$capture_dir/docs-stills/start-task-dark.webp" --at 12.5 --width 1600
```

`still` also accepts `--crop X Y W H` using original pixels. It verifies the 4K video source and keeps a separate provenance sidecar. Put reviewed article stills in `site/public/screens/native/` and article videos in `site/public/media/native/`, each with both theme suffixes. Do not change article paths until the matching files exist. The article screenshots need no fake-daemon mode; prepare the corresponding real demo state instead.

For a full-Dell 3840×2160 PNG screenshot, use the image path. It uses the existing `cwebp` encoder and retains a separate provenance sidecar:

```sh
bash scripts/site-capture/native-record.sh image "$capture_dir/start-task-dark.png" "$capture_dir/docs-stills/start-task-dark.webp" --crop 0 0 3840 2160 --width 1600
```
