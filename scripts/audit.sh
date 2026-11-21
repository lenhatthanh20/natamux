#!/bin/sh
# Advisory checks that cargo deny and a plain cargo audit do not do. Run in CI only.
#   audit.sh        cargo audit, then again with the git crates
#   audit.sh zed    new security advisories of Zed upstream (needs gh and GH_TOKEN)
#   audit.sh osv [lock]  crates that OSV lists as malicious (Linux x64 runner)
set -eu
cd "$(dirname "$0")/.."

if [ "${1:-}" = zed ]; then
  ids=$(gh api --paginate 'repos/zed-industries/zed/security-advisories?per_page=100' --jq '.[].ghsa_id')
  known=$(grep -o '^GHSA[^ ]*' supply-chain/zed-advisories.txt)
  new=$(echo "$ids" | grep -vxF "$known" || true)
  [ -z "$new" ] && exit 0
  echo "audit: Zed advisories not in supply-chain/zed-advisories.txt:" >&2
  echo "$new" >&2
  exit 1
fi

if [ "${1:-}" = osv ]; then
  # OSV also holds the OpenSSF list of malicious packages (MAL- ids), which RustSec does not.
  # Only those ids fail here: deny.toml already rules on the other advisories.
  tmp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
  curl -sSfL -o "$tmp/osv-scanner" https://github.com/google/osv-scanner/releases/download/v2.7.1/osv-scanner_linux_amd64
  echo "fb7dab19a8bd9bbe5c3189f09d4b7ee1820c9553a23bac202648a56efd2f6ee8  $tmp/osv-scanner" | sha256sum -c -
  chmod +x "$tmp/osv-scanner"
  code=0
  "$tmp/osv-scanner" scan source --lockfile "${2:-Cargo.lock}" --format json > "$tmp/osv.json" || code=$?
  # 1 means that it found advisories; any other code is a scan that did not work.
  [ "$code" -le 1 ] || { echo "audit: osv-scanner failed with $code" >&2; exit 1; }
  grep -q '"results"' "$tmp/osv.json" || { echo "audit: osv-scanner gave no report" >&2; exit 1; }
  bad=$(grep -o '"MAL-[0-9A-Za-z-]*"' "$tmp/osv.json" | sort -u || true)
  [ -z "$bad" ] && exit 0
  echo "audit: OSV lists a crate of the lock as malicious:" >&2
  echo "$bad" >&2
  exit 1
fi

cargo audit
# cargo audit and cargo deny skip a crate that comes from git, so a known hole in a fork stays
# unseen. Check a copy of the lock that says those crates come from crates.io: same name and
# version, same advisories.
lock="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/Cargo.audit.lock"
sed -E 's#^source = "git\+[^"]*"#source = "registry+https://github.com/rust-lang/crates.io-index"#; s# \(git\+[^)]*\)##' Cargo.lock > "$lock"
cargo audit --file "$lock"
