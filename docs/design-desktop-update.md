# Design: desktop app updates

The desktop app finds a new release, downloads it in the background, and installs it when you restart, the way other desktop apps do. Build order step 11, the update part. Signing comes later.

## Why not electron-updater

- On macOS, electron-updater installs through Squirrel.Mac. Squirrel installs a new bundle only when it satisfies the code requirement of the running app. An ad hoc signature (the app has no Developer ID yet) names the hash of one build, so no later build satisfies it. Updates would fail on every Mac.
- It needs a `zip` target, `latest-*.yml` files, and a merge step for the two Mac builds in CI.
- The app's own updater needs no new package and works the same before and after signing.

## Where the update comes from

- Feed: `HEAD https://github.com/lenhatthanh20/natamux/releases/latest`. GitHub redirects it to `/releases/tag/<tag>` of the newest release that is not a draft and not a pre-release. The app reads the tag from the address it ends at. When the repository moves, GitHub redirects the old address too.
- Not the REST API (`api.github.com`): it allows 60 requests an hour for each address without a login, and shared networks and CI runners reach that. The dry run in CI got `403` from it.
- Files: `https://github.com/<owner>/<repo>/releases/download/<tag>/<name>`, from the owner and repository in the address the redirect ends at.
- The release must carry `SHA256SUMS.txt` (`release.yml` writes it) and the file for the platform:

| Install kind | How the app finds it | File |
|---|---|---|
| `mac` | `process.platform === 'darwin'` | `Natamux-mac-arm64.dmg` or `Natamux-mac-x64.dmg`, from `process.arch` |
| `appimage` | Linux, `APPIMAGE` is set | `Natamux-linux-x86_64.AppImage` |
| `deb` | Linux, the app runs from `/opt/Natamux/` | `Natamux-linux-amd64.deb` |
| `manual` | anything else: the `.tar.gz`, Windows, a Mac app run from a disk image or from a folder the user cannot write | none: the app opens the release page |

- A Mac app counts as `manual` when its path holds `/AppTranslocation/` or starts with `/Volumes/`, or when the user cannot write the folder that holds `Natamux.app`.
- An AppImage counts as `manual` when the user cannot write its folder.

## When it checks

- 30 seconds after the app starts, then every 6 hours while it runs.
- **Check for Updates…** in the menu checks now. It is on the app menu on macOS and on the **Help** menu on Linux and Windows.
- **Check for Updates Automatically** (a tick in the same menu) turns the timed checks off and on. It is saved as `autoUpdate` in `<user data>/settings.json`, on by default. A manual check always works.
- No check when the app is not packaged (`pnpm start`) or in smoke mode (`NATAMUX_DESKTOP_SMOKE=1`).

## What counts as newer

- Versions are `MAJOR.MINOR.PATCH`, with an optional `v` in front. A tag with anything else, such as `-beta.1`, is never offered.
- Newer means a higher number, compared part by part with `app.getVersion()`.
- `NATAMUX_UPDATE_AS_VERSION=0.0.0` makes the app act as that version, so a release can update to itself. For tests only.

## States

```
idle → checking → up-to-date
                → available → downloading → ready → (restart) → installing
                                          → failed
        any step → failed
```

- `available` with kind `manual`: the app shows a dialog with **Open the Release Page** and **Later**. It downloads nothing.
- `available` with any other kind: the download starts at once.
- `ready`: the app shows a dialog: "Natamux 0.1.2 is ready. Restart to update." with **Restart Now**, **Later** and **What's New** (opens the release page). The menu item becomes **Restart to Update to 0.1.2**.
- A version the user answered with **Later** asks again only after the next start. The menu item stays.
- A timed check that fails stays quiet; the next timed check tries again. A manual check shows the error.

## Download and checks

- Folder: `<user data>/updates/<version>/`. Before a download the app removes the other version folders there.
- The file downloads to `<name>.part` with `net.fetch`, which uses the system proxy, then is renamed.
- The app reads `SHA256SUMS.txt` from the same release and compares the SHA-256 of the file. A missing line or a different hash removes the file and fails.
- `mac` only:
  1. `hdiutil attach -nobrowse -readonly -mountpoint <folder>/mnt <dmg>`;
  2. `ditto` the `Natamux.app` from it to `<folder>/Natamux.app`;
  3. `hdiutil detach`;
  4. `codesign --verify --deep --strict` on the copy;
  5. `CFBundleShortVersionString` of the copy must be the new version.

  A file the app downloads itself gets no quarantine flag, so macOS does not ask again when the new version opens.
- `appimage`: the file goes to `<AppImage folder>/.Natamux-<version>.AppImage.part`, gets mode `0755`, and is renamed over the running AppImage once it is checked. Linux keeps the old file open for the running app. The next start runs the new version, even without a restart.

## Install on restart

**Restart Now** asks first when agents are working, as quitting does. Then:

| Kind | What happens |
|---|---|
| `mac` | The app writes `<folder>/install.sh` and starts it detached with `/bin/sh`. The script waits until the app's process is gone. It renames the old bundle to `Natamux.app.old`, moves the new one in, removes the old one, and runs `open` on the bundle. If the move fails, it puts the old bundle back and opens it. |
| `appimage` | The file is in place already. The app restarts with `APPIMAGE` as the program. |
| `deb` | The app runs `pkexec dpkg -i <file>` and waits. On success it restarts. When `pkexec` is missing or the password prompt is closed, it shows the command to run by hand: `sudo apt install <file>`. |

- `mac` also installs on a normal quit when an update is ready, without starting the app again.
- The script writes what it did to `<user data>/logs/update.log`.

## Log

`<user data>/logs/update.log` gets one line for each step: check, version found, download start and end, hash result, install start, and every failure with its reason.

## Security

- Everything comes over HTTPS from GitHub. The checksum file comes from the same release, so it finds a broken download, not a forged release. Someone who can publish a release can publish a matching checksum.
- After signing (step 11): the Mac copy must also be signed by the same Team ID before the app installs it.

## Privacy

The check is one `HEAD` request to `github.com` without any data about you or your tasks. GitHub sees your IP address and the app's version in the `User-Agent`. **Check for Updates Automatically** turns it off. `PRIVACY.md` says so.

## Files

- `packages/desktop/src/update-check.ts`: no Electron: version compare, install kind, file pick, checksum parse, the install script.
- `packages/desktop/src/updater.ts`: the Electron side: timers, network, dialogs, menu state, install.
- `packages/desktop/src/main.ts`: the menu items and the quit hook.
- `packages/desktop/src/settings.ts`: `autoUpdate`.
