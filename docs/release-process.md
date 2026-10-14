# Release process

How a version gets from `main` to users. The desktop app installs an update only when `SHA256SUMS.txt` of the release is signed with the release key, so every release has a signing step on the owner's Mac.

## Steps

1. On a branch: set the version in `packages/{cli,core,daemon,desktop,web}/package.json` and the homepage, write `docs/releases/vX.Y.Z.md` and the changelog section (the docs skill says how). `scripts/release.test.sh` must pass. Merge to `main`.
2. Tag the merge commit with an annotated tag and push it:

   ```sh
   git tag -a vX.Y.Z -m "Natamux vX.Y.Z" <commit on main>
   git push origin vX.Y.Z
   ```

3. The Release workflow (`.github/workflows/release.yml`) builds and starts the app on every system, writes `SHA256SUMS.txt`, and makes a **draft** release with every file. `/releases/latest` skips drafts, so no app and no download link sees it yet.
4. On the Mac that holds the secret key, after the run is green:

   ```sh
   scripts/sign-release.sh vX.Y.Z
   ```

   It checks that the release is a draft, the tag is annotated and on `main`, the Release run of the tag succeeded and built that commit, and `SHA256SUMS.txt` lists exactly the files of the release with the SHA-256 digests GitHub computed. It shows the commit, the run and the files, and asks. On `y` it signs `SHA256SUMS.txt` in minisign's legacy form (`-l`) with the trusted comment `natamux vX.Y.Z`, checks the signature against `RELEASE_SIGNER`, uploads `SHA256SUMS.txt.minisig`, and publishes the draft as the latest release. minisign asks for the key's password. It works in a new `mktemp -d` folder and leaves it there.
5. Check the published release: `scripts/release.test.sh --published` (needs `minisign`).

Never publish the draft by hand: a release without `SHA256SUMS.txt.minisig` updates no app, and timed checks keep failing until it is signed.

## The release key

- A [minisign](https://jedisct1.github.io/minisign/) key pair, made once on the owner's Mac: `minisign -G -p ~/.minisign/natamux.pub -s ~/.minisign/natamux.key`. The secret key has a password, stays on that Mac and an offline backup, and is never on GitHub or in CI. `NATAMUX_MINISIGN_SECRET` points the script at another file.
- The public key (key id `3F449A1E170BB7A0`) is `RELEASE_SIGNER` in `packages/desktop/src/release-signature.ts`, and it is printed on the install page, the security page and in `SECURITY.md`. The CI job `release-signer` fails when it is not a minisign key, when it is a test key, or when one of those pages does not show it.

  ```
  RWSgtwsXHppEP5n4kgg1Q3yNIYODQNGsMQBKF93tjSRwzL1BrjeGoBpo
  ```

## What the app checks

Before it trusts any hash in `SHA256SUMS.txt`, the app downloads `SHA256SUMS.txt.minisig` from the same release and checks, with Node's crypto only (`packages/desktop/src/release-signature.ts`):

- the signature file has the four minisign lines, and its algorithm is `Ed`, the legacy form: Ed25519 over the file itself. The default, prehashed form `ED` signs the BLAKE2b-512 of the file, and Electron's crypto (BoringSSL) has no BLAKE2b-512, so the app refuses it. CI proved this: the smoke check failed with `Digest method not supported` for `ED` and passed for `Ed`;
- its key id is the id of `RELEASE_SIGNER`;
- the Ed25519 signature of `SHA256SUMS.txt` is valid for `RELEASE_SIGNER`;
- the signature of the trusted comment is valid;
- the trusted comment is exactly `natamux <tag>`, where `<tag>` is the release `/releases/latest` led to;
- that tag's version is newer than the running app.

Any failure, including a missing `.minisig`, stops the update before the download. A manual **Check for Updates…** shows the reason; a timed check writes it to `logs/update.log`. There is no fallback to the checksum alone.

The comment check ties a signature to one release. An old signed `SHA256SUMS.txt` uploaded under a new tag fails, so a stolen GitHub account cannot push an old, vulnerable version as an update.

## Limits

- Versions up to v0.1.23 do not check the signature. They still trust `SHA256SUMS.txt` of the release alone. Only versions with this check are protected, so the first signed release should ship soon, and users should update to it.
- The key is in the app. To change it, ship a release with the new key, signed with the old one. A leaked key lets its holder sign updates until every app runs a version with a new key. A lost key means apps cannot update themselves any more; users must download a new version by hand.
- Windows copies do not update themselves; users check their download as the [install page](https://natamux.github.io/docs/getting-started/install/#verify-the-download) says.
