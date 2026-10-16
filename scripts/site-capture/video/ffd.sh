#!/usr/bin/env bash
# ffd.sh <dir> <ffmpeg args...>: runs ffmpeg in Docker with <dir> copied to /work (the working dir);
# files written under /work/out come back to <dir>/out. Paths in args are relative to <dir>.
set -euo pipefail
dir=$1; shift
id=$(docker create -w /work linuxserver/ffmpeg:latest -hide_banner -loglevel error -y "$@")
trap 'docker rm -f "$id" >/dev/null' EXIT
mkdir -p "$dir/out"
docker cp "$dir/." "$id:/work" >/dev/null
docker start -a "$id"
docker cp "$id:/work/out/." "$dir/out" >/dev/null
