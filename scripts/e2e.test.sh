#!/usr/bin/env bash
# Usage: scripts/e2e.test.sh
# Checks how scripts/e2e.sh picks the run whose images E2E_RUN loads, with stubs for gh and docker.
set -u
fails=0
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
stubs="$tmp/bin"
mkdir "$stubs"
log="$tmp/calls"

# Run 1 is an old E2E run, 2 another workflow, 3 a fork, 4 has no artifact, 5 does not load,
# and 6 is a Rust production-image run.
cat >"$stubs/gh" <<'STUB'
#!/usr/bin/env bash
echo "gh $*" >>"$CALLS"
case "$1 $2" in
'api repos/lenhatthanh20/natamux/actions/runs/1' | 'api repos/lenhatthanh20/natamux/actions/runs/4' | \
  'api repos/lenhatthanh20/natamux/actions/runs/5')
  echo '.github/workflows/e2e.yml lenhatthanh20/natamux' ;;
'api repos/lenhatthanh20/natamux/actions/runs/2') echo '.github/workflows/ci.yml lenhatthanh20/natamux' ;;
'api repos/lenhatthanh20/natamux/actions/runs/3') echo '.github/workflows/e2e.yml someone/natamux' ;;
'api repos/lenhatthanh20/natamux/actions/runs/6') echo '.github/workflows/rust.yml lenhatthanh20/natamux' ;;
'run download')
  [ "$3" = 1 ] || [ "$3" = 5 ] || [ "$3" = 6 ] || exit 1
  touch "${@: -1}/images.tar.zst" ;;
*) exit 1 ;;
esac
STUB
cat >"$stubs/docker" <<'STUB'
#!/usr/bin/env bash
echo "docker $*" >>"$CALLS"
! grep -q '^gh run download 5 ' "$CALLS"
STUB
chmod +x "$stubs/gh" "$stubs/docker"

# load <run>: load_run in a subshell; prints its exit code.
load() {
  : >"$log"
  (
    # shellcheck source=scripts/e2e.sh
    source "$(dirname "$0")/e2e.sh"
    load_run "$1"
  ) >/dev/null 2>"$tmp/err"
  echo $?
}

check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

export PATH="$stubs:$PATH" CALLS="$log"

code=$(load 1)
loaded=$(sed -n 's/^docker load -i //p' "$log")
check "loads the images of an E2E run of the repository" '[ "$code" = 0 ] && [ -n "$loaded" ]'
check "downloads the images artifact of that run" \
  'grep -q "^gh run download 1 --repo lenhatthanh20/natamux -n images -D " "$log"'
check "removes the downloaded file after the load" '[ ! -e "$loaded" ]'

code=$(load 2)
check "refuses a run of another workflow" '[ "$code" = 1 ] && grep -q "not an E2E run" "$tmp/err"'
check "downloads nothing for it" '! grep -q "run download" "$log"'

code=$(load 6)
check "loads the Rust production images from a Rust CI run" \
  '[ "$code" = 0 ] && grep -q "^gh run download 6 --repo lenhatthanh20/natamux -n rust-production-images -D " "$log"'

code=$(load 3)
check "refuses a run from a fork" '[ "$code" = 1 ] && ! grep -q "run download" "$log"'

code=$(load 4)
check "says when the artifact is gone" \
  '[ "$code" = 1 ] && grep -q "no images artifact" "$tmp/err" && ! grep -q "^docker" "$log"'

code=$(load 5)
loaded=$(sed -n 's/^docker load -i //p' "$log")
check "removes the downloaded file when the load fails" \
  '[ "$code" = 1 ] && grep -q "could not load" "$tmp/err" && [ -n "$loaded" ] && [ ! -e "$loaded" ]'

# compose <extra file or empty>: the docker command line compose() builds.
compose_line() {
  : >"$log"
  (
    # shellcheck source=scripts/e2e.sh
    source "$(dirname "$0")/e2e.sh"
    E2E_EXTRA_COMPOSE=$1 compose ps
  ) >/dev/null 2>&1
  cat "$log"
}

check "adds the extra compose file on top" \
  '[ "$(compose_line compose.rust-cli.yaml)" = "docker compose -p natamux-e2e -f compose.prod.yaml -f compose.e2e.yaml -f compose.rust-cli.yaml ps" ]'
check "adds no file without one" \
  '[ "$(compose_line "")" = "docker compose -p natamux-e2e -f compose.prod.yaml -f compose.e2e.yaml ps" ]'

if [ "$fails" -gt 0 ]; then
  echo "$fails failed"
  exit 1
fi
echo "all passed"
