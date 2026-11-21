#!/bin/sh
# Advisory checks that cargo deny and a plain cargo audit do not do. Run in CI only.
#   audit.sh        cargo audit, then again with the git crates
#   audit.sh zed    new security advisories of Zed upstream (needs gh and GH_TOKEN)
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

cargo audit
# cargo audit and cargo deny skip a crate that comes from git, so a known hole in a fork stays
# unseen. Check a copy of the lock that says those crates come from crates.io: same name and
# version, same advisories.
lock="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/Cargo.audit.lock"
sed -E 's#^source = "git\+[^"]*"#source = "registry+https://github.com/rust-lang/crates.io-index"#; s# \(git\+[^)]*\)##' Cargo.lock > "$lock"
cargo audit --file "$lock"
