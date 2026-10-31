#!/usr/bin/env bash
# Usage: scripts/make-mac-sign-cert.sh [<out-dir>]
# For the owner only, once. Makes the self-signed certificate that release runs sign the macOS
# app with, so macOS keeps Full Disk Access and other privacy grants after an update. Writes
# natamux-codesign.p12 and its password file into <out-dir> (default: a new mktemp -d folder),
# never into the repository, and prints the SHA-1 to pin and the commands that store the secrets.
# Keep both files in 1Password. See docs/release-process.md.
set -euo pipefail

# The macOS LibreSSL writes the legacy .p12 form that `security import` reads; OpenSSL 3 does not.
openssl=/usr/bin/openssl
out=${1:-$(mktemp -d)}
mkdir -p "$out"
cd "$out"
[ ! -e natamux-codesign.p12 ] || { echo "make-mac-sign-cert: $out/natamux-codesign.p12 exists" >&2; exit 1; }
umask 077

cat > codesign.cnf <<'CNF'
[req]
distinguished_name = dn
prompt = no
x509_extensions = ext
[dn]
CN = Natamux Code Signing
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF
"$openssl" req -x509 -newkey rsa:2048 -nodes -days 7300 -config codesign.cnf \
  -keyout codesign-private.pem -out codesign.pem 2>/dev/null
"$openssl" rand -base64 24 | tr -d '\n' > p12-password.txt
"$openssl" pkcs12 -export -name "Natamux Code Signing" -inkey codesign-private.pem -in codesign.pem \
  -out natamux-codesign.p12 -passout file:p12-password.txt
rm -f codesign-private.pem codesign.cnf
sha1=$("$openssl" x509 -in codesign.pem -noout -fingerprint -sha1 | sed 's/.*=//; s/://g' \
  | tr '[:upper:]' '[:lower:]')

cat <<EOF
Made $out/natamux-codesign.p12 and $out/p12-password.txt.
1. Pin the SHA-1 in scripts/package-rust-desktop.sh:
   mac_sign_sha1="$sha1"
2. Store the secrets:
   base64 -i "$out/natamux-codesign.p12" | gh secret set MACOS_SIGN_P12
   gh secret set MACOS_SIGN_P12_PASSWORD < "$out/p12-password.txt"
3. Save both files in 1Password, then delete the folder.
EOF
