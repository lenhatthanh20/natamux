#!/usr/bin/env bash
# ffm.sh <input> <output> [ffmpeg options...]: runs ffmpeg in Docker on a local file. colima may
# not mount the temp folder, so the files go in and out with docker cp instead of a volume.
set -euo pipefail
in=$1 out=$2; shift 2
image=${FFMPEG_IMAGE:-linuxserver/ffmpeg:latest}
ext_in=${in##*.} ext_out=${out##*.}
id=$(docker create "$image" -hide_banner -loglevel error -y -i "/tmp/in.$ext_in" "$@" "/tmp/out.$ext_out")
trap 'docker rm -f "$id" >/dev/null' EXIT
docker cp "$in" "$id:/tmp/in.$ext_in" >/dev/null
docker start -a "$id"
docker cp "$id:/tmp/out.$ext_out" "$out" >/dev/null
echo "$out"
