#!/usr/bin/env bash
# Builds the shop and copies it to the server.
set -euo pipefail

TARGET="${1:-staging}"
readonly BUILD_DIR="dist"
count=0

log() {
  printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*" >&2
}

if [[ ! -d "$BUILD_DIR" ]]; then
  log "no build yet, building"
  make build
fi

for file in "$BUILD_DIR"/*.js; do
  count=$((count + 1))
  gzip -k9 "$file"
done

case "$TARGET" in
  staging) host=stage.example.com ;;
  production) host=www.example.com ;;
  *) log "unknown target: $TARGET"; exit 2 ;;
esac

rsync -az --delete "$BUILD_DIR/" "deploy@$host:/srv/shop" | tee -a deploy.log
log "sent $count files to $host"
