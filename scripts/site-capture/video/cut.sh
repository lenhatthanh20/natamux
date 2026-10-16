#!/usr/bin/env bash
# cut.sh <out> <src.webm> <w:h> <crf> <still start s> <still end s> "s1-e1 s2-e2 ..."
# keeps the given ranges of the raw take (seconds), joins them, adds still holds, encodes H.264.
set -euo pipefail
out=$1 src=$2 size=$3 crf=$4 a=$5 b=$6; read -r -a R <<<"$7"
n=${#R[@]} f="[0:v]fps=30,split=$n"
for i in "${!R[@]}"; do f+="[a$i]"; done; f+=";"
cat=""
for i in "${!R[@]}"; do s=${R[$i]%-*} e=${R[$i]#*-}; f+="[a$i]trim=start=$s:end=$e,setpts=PTS-STARTPTS[b$i];"; cat+="[b$i]"; done
f+="${cat}concat=n=$n:v=1:a=0,scale=$size:flags=lanczos,setsar=1,tpad=start_mode=clone:start_duration=$a:stop_mode=clone:stop_duration=$b,format=yuv420p"
codec=(-an -c:v libx264 -preset slow -crf "$crf" -pix_fmt yuv420p -movflags +faststart)
case $out in *.mkv) codec=(-an -c:v libx264 -preset ultrafast -crf 8 -pix_fmt yuv420p) ;; esac
"$(dirname "$0")/../ffm.sh" "$src" "$out" -filter_complex "$f" "${codec[@]}" >/dev/null
printf '%s %s bytes %s frames\n' "$(basename "$out")" "$(stat -f %z "$out")" "$(docker run -i --rm --entrypoint ffprobe linuxserver/ffmpeg:latest -v error -count_frames -show_entries stream=nb_read_frames -of csv=p=0 -i pipe:0 < "$out")"
