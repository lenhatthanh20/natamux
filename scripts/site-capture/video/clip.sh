#!/usr/bin/env bash
# clip.sh <out.mp4> <src.webm> <start s> <frames> <still start s> <still end s> <crf> [w:h]
set -euo pipefail
out=$1 src=$2 ss=$3 n=$4 a=$5 b=$6 crf=$7 size=${8:-1600:1000}
"$(dirname "$0")/../ffm.sh" "$src" "$out" -ss 0 -vf "trim=start=$ss,setpts=PTS-STARTPTS,fps=30,trim=end_frame=$n,setpts=PTS-STARTPTS,scale=$size:flags=lanczos,setsar=1,tpad=start_mode=clone:start_duration=$a:stop_mode=clone:stop_duration=$b,format=yuv420p" \
  -an -c:v libx264 -preset slow -crf "$crf" -pix_fmt yuv420p -movflags +faststart >/dev/null
printf '%s %s bytes, %s\n' "$(basename "$out")" "$(stat -f %z "$out")" "$(docker run -i --rm --entrypoint ffprobe linuxserver/ffmpeg:latest -v error -show_entries format=duration -of csv=p=0 -i pipe:0 < "$out")"
