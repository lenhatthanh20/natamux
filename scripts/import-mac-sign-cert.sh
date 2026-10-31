#!/usr/bin/env bash
# Usage: scripts/import-mac-sign-cert.sh <file.p12> <password-file> <keychain>
# For CI on macOS. Imports the code signing certificate into a new keychain, adds that keychain to
# the search list so codesign finds it, and prints the certificate's SHA-1, the identity that
# scripts/package-rust-desktop.sh signs with.
set -euo pipefail

p12=${1:?usage: scripts/import-mac-sign-cert.sh <file.p12> <password-file> <keychain>}
password_file=${2:?missing password file}
keychain=${3:?missing keychain}
# Only this job uses the keychain, so a random password nobody keeps is enough.
keychain_password=$(openssl rand -hex 16)

security create-keychain -p "$keychain_password" "$keychain" >&2
security set-keychain-settings "$keychain" >&2
security unlock-keychain -p "$keychain_password" "$keychain" >&2
security import "$p12" -k "$keychain" -f pkcs12 -P "$(cat "$password_file")" -T /usr/bin/codesign >&2
# Lets codesign use the private key without a dialog.
security set-key-partition-list -S apple-tool:,apple: -s -k "$keychain_password" "$keychain" >/dev/null
# The login keychain paths have no spaces.
# shellcheck disable=SC2046
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
security find-certificate -a -Z -c "Natamux Code Signing" "$keychain" \
  | sed -n 's/^SHA-1 hash: //p' | head -n 1 | tr '[:upper:]' '[:lower:]'
