# Release process

How a version gets from `main` to users. The desktop app installs an update only when `SHA256SUMS.txt` of the release is signed with the release key, so every release has a signing step on the owner's Mac.

## One command

Once per Mac, run `scripts/setup-release-signing.sh` (see [The tag key](#the-tag-key)). Then, after the release pull request (step 1) is merged:

```sh
git fetch --no-tags origin main && git switch --detach origin/main
scripts/release.sh vX.Y.Z
```

It does steps 2 to 5: it asks before it makes and pushes the signed tag, waits for the Release run, and runs `scripts/sign-release.sh`, which asks y/N and the minisign password. Run it again with the same tag if it stops after the tag is pushed.

## Steps

1. On a branch: set the version in every `crates/*/Cargo.toml`, `Cargo.lock`, `packages/{core,web}/package.json`, the macOS app bundle, and the homepage. Write `docs/releases/vX.Y.Z.md` and the changelog section (the docs skill says how). `scripts/release.test.sh` and a Release dry run with `publish=false` must pass in CI. Merge to `main`.
2. Tag the merge commit with a tag signed by your SSH key (see [The tag key](#the-tag-key)) and push it:

   ```sh
   git tag -s vX.Y.Z -m "Natamux vX.Y.Z" <commit on main>
   git push origin vX.Y.Z
   ```

3. The Release workflow (`.github/workflows/release.yml`) builds and starts the app on every system, writes `SHA256SUMS.txt`, and makes a **draft** release with every file. `/releases/latest` skips drafts, so no app and no download link sees it yet.
4. On the Mac that holds the secret key and the tag you made, after the run is green and **within 3 days** of it:

   ```sh
   scripts/sign-release.sh vX.Y.Z
   ```

   It checks that:
   - the tag is `vX.Y.Z`, signed by a key in your allowed signers file (`git verify-tag`), and the same object here and on GitHub. It trusts your local, signed tag, not GitHub's: someone who can push could move the tag there, and a `git fetch` can bring their tag here, but they cannot sign it with your key;
   - the tag's commit is on `main`;
   - the release is a draft, not marked as a pre-release, with exactly the six files and `SHA256SUMS.txt`, and `SHA256SUMS.txt` lists them with the SHA-256 digests GitHub computed;
   - the push Release run of the tag succeeded and built that commit;
   - the build artifacts of that run hash to exactly the lines of `SHA256SUMS.txt`. A stolen token can replace release files and `SHA256SUMS.txt` together, but not the artifacts of a finished run. The artifacts last 3 days (`retention-days` in `release.yml`). After that the script stops; re-run the Release workflow, then sign within 3 days.

   It shows the commit, the run, `SHA256SUMS.txt`, and from your local repository the commits and the changed files since the previous release tag. Read them, then answer. On `y` it signs `SHA256SUMS.txt` in minisign's legacy form (`-l`) with the trusted comment `natamux vX.Y.Z`, checks the signature against `RELEASE_SIGNER`, uploads `SHA256SUMS.txt.minisig`, and publishes the draft as the latest release. minisign asks for the key's password. It works in a new `mktemp -d` folder and leaves it there.
5. Check the published release: `scripts/release.test.sh --published` (needs `minisign`).
6. The first Rust release only: on a Mac that runs the last Electron app (v0.1.43), choose **Check for Updates…**. It must find the new release, install the Rust app over `Natamux.app`, and restart into it with the same tasks and sign-in. CI checks the DMG name and the bundle the Electron updater expects, but only a signed release shows the whole update.

Never publish the draft by hand: a release without `SHA256SUMS.txt.minisig` updates no app, and timed checks keep failing until it is signed.

Do not re-run the Release workflow once signing has started. A re-run replaces the files and `SHA256SUMS.txt` of the draft (`--clobber`), so a signature made before it no longer matches and apps refuse the release. That fails closed: sign again.

## The release key

- A [minisign](https://jedisct1.github.io/minisign/) key pair, made once on the owner's Mac: `minisign -G -p ~/.minisign/natamux.pub -s ~/.minisign/natamux.key`. The secret key has a password, stays on that Mac and an offline backup, and is never on GitHub or in CI. `NATAMUX_MINISIGN_SECRET` points the script at another file.
- The public key (key id `3F449A1E170BB7A0`) is `RELEASE_SIGNER` in `crates/natamux-update/src/signature.rs`, and it is printed on the install page, the security page and in `SECURITY.md`. The release checks fail when it is not the pinned minisign key.

  ```
  RWSgtwsXHppEP5n4kgg1Q3yNIYODQNGsMQBKF93tjSRwzL1BrjeGoBpo
  ```

## The tag key

Release tags are signed with the owner's SSH key. Set it up once:

```sh
scripts/setup-release-signing.sh             # ~/.ssh/id_ed25519.pub
scripts/setup-release-signing.sh <file.pub>  # another key
```

It sets this repository to sign tags with the key (`gpg.format ssh`, `user.signingkey`), adds the key to `~/.ssh/allowed_signers` when it is not there, and signs and checks a tag in a new, empty repository.

`scripts/sign-release.sh` checks the tag against `~/.ssh/allowed_signers` (or the file in `NATAMUX_TAG_SIGNERS`). Keep that file outside the repository: anyone who can push could add a key to a file in it.

## What the app checks

Before it trusts any hash in `SHA256SUMS.txt`, the Rust app downloads `SHA256SUMS.txt.minisig` from the same release and checks it with `natamux-update`:

- the signature file has the four minisign lines, and its algorithm is `Ed`, the legacy form: Ed25519 over the file itself. The last Electron release also needs this form to read the new Rust release;
- its key id is the id of `RELEASE_SIGNER`;
- the Ed25519 signature of `SHA256SUMS.txt` is valid for `RELEASE_SIGNER`;
- the signature of the trusted comment is valid;
- the trusted comment is exactly `natamux <tag>`, where `<tag>` is the release `/releases/latest` led to;
- that tag's version is newer than the running app.

Any failure, including a missing `.minisig`, stops the update before the download. The check runs before the app offers anything: a copy that cannot update itself (Windows, a Mac app run from the disk image) is not even shown the release page of an unsigned release. `SHA256SUMS.txt` and its signature larger than 64 KB are refused. A manual **Check for Updates…** shows the reason; a timed check writes it to `logs/update.log`. There is no fallback to the checksum alone.

The comment check ties a signature to one release. An old signed `SHA256SUMS.txt` uploaded under a new tag fails, so a stolen GitHub account cannot push an old, vulnerable version as an update.

## Limits

- Versions up to v0.1.23 do not check the signature. They still trust `SHA256SUMS.txt` of the release alone. Only versions with this check are protected, so the first signed release should ship soon, and users should update to it.
- The key is in the app. To change it, ship a release with the new key, signed with the old one. A leaked key lets its holder sign updates until every app runs a version with a new key. A lost key means apps cannot update themselves any more; users must download a new version by hand.
- Windows copies do not update themselves; users check their download as the [install page](https://natamux.github.io/docs/getting-started/install/#verify-the-download) says.
- The signature proves the files are what the Release run built from the tagged commit. It does not prove the build is clean: a bad action or a bad dependency inside the build makes artifacts that pass every check. Only an independent rebuild would catch that. Read the change list the script shows.
- Freeze: GitHub, or someone who controls the repository, can keep serving an older signed release as the latest. Apps then see no update. They never go back to an older version, but they can miss a fix.
- `main` and the `v*` tags have no branch or tag protection today. Turn on branch protection for `main` and a tag rule for `v*` in the repository settings, so a stolen token cannot push to `main` or move a release tag. The script's signed-tag check covers the tag, not what is on `main`.
