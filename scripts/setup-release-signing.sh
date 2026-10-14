#!/usr/bin/env bash
# Usage: scripts/setup-release-signing.sh [ssh-public-key-file]
# For the owner only, once per Mac. Sets this repository to sign tags with your SSH key (default
# ~/.ssh/id_ed25519.pub), adds the key to ~/.ssh/allowed_signers (or NATAMUX_TAG_SIGNERS) when it
# is not there, and checks that a signed tag passes the check of scripts/sign-release.sh. Changes
# nothing else and removes nothing. See docs/release-process.md.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=sign-release.sh
source scripts/sign-release.sh

fail() {
  echo "setup-release-signing: $1" >&2
  exit 1
}

public=${1:-$HOME/.ssh/id_ed25519.pub}
signers=${NATAMUX_TAG_SIGNERS:-$HOME/.ssh/allowed_signers}
for tool in gh git minisign ssh-keygen shasum; do
  command -v "$tool" >/dev/null || fail "$tool is not installed (minisign: brew install minisign)"
done
gh auth status >/dev/null 2>&1 || fail "gh is not signed in: gh auth login"
[ -f "$public" ] || fail "no SSH public key at $public; pass its file: scripts/setup-release-signing.sh <file.pub>"
[ -f "${NATAMUX_MINISIGN_SECRET:-$HOME/.minisign/natamux.key}" ] \
  || fail "no minisign secret key at ${NATAMUX_MINISIGN_SECRET:-~/.minisign/natamux.key}"
email=$(git config user.email || true)
[ -n "$email" ] || fail "set your email first: git config user.email you@example.com"

# The private key file signs without ssh-agent; ssh-keygen asks its passphrase if it has one.
signing=$public
[ -f "${public%.pub}" ] && signing=${public%.pub}
git config gpg.format ssh
git config user.signingkey "$signing"
key=$(cut -d ' ' -f 1,2 "$public")
if grep -qF "$key" "$signers" 2>/dev/null; then
  echo "$signers lists the key already."
else
  echo "$email $key" >>"$signers"
  echo "Added your key to $signers."
fi

# A signed tag in a new, empty repository, checked the way sign-release.sh checks a release tag.
try=$(mktemp -d)
git init -q "$try"
git -C "$try" -c user.name=Natamux -c user.email="$email" commit -q --allow-empty -m try
git -C "$try" -c gpg.format=ssh -c user.signingkey="$signing" tag -s v0.0.0 -m try \
  || fail "could not sign a tag with $signing"
(cd "$try" && tag_signed v0.0.0 "$signers") || fail "a tag signed with $public does not pass the check"
echo "Release signing is ready. Release with: scripts/release.sh vX.Y.Z"
