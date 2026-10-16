#!/usr/bin/env bash
# film.sh <theme> <crf> s1 s2 s3 s4 s5 s6: cuts six chapters to fixed frame counts and joins them with a
# 0.6 s still end. Needs the raw takes ch1-<theme>.webm, live-<theme>.webm and agents-<theme>.webm in
# $OUT/enc/film. Writes $OUT/enc/film/cat/out/film-<theme>.mp4 and prints the frames of each chapter.
# s1..s6 are the start seconds of each chapter in its take. Frame counts must match site/src/data/film.json.
set -euo pipefail
t=$1 crf=$2; shift 2
N=(222 195 195 210 195 128)
src=(ch1 live live live agents live)
here=$(cd "$(dirname "$0")" && pwd)
OUT=${OUT:-./out}
dir="$OUT/enc/film"
mkdir -p "$dir/out"
args=() maps=()
for i in 0 1 2 3 4 5; do
  args+=(-ss "$1" -t 8.5 -i "${src[$i]}-$t.webm"); shift
  maps+=(-map "[v$i]" -c:v libx264 -preset ultrafast -crf 8 -pix_fmt yuv420p "out/seg-$t-$i.mkv")
  f+="[$i:v]fps=30,trim=end_frame=${N[$i]},setpts=PTS-STARTPTS,scale=1920:1200:flags=lanczos,setsar=1,format=yuv420p[v$i];"
done
"$here/ffd.sh" "$dir" "${args[@]}" -filter_complex "${f%;}" "${maps[@]}"
mkdir -p "$dir/cat"; : > "$dir/cat/list.txt"
for i in 0 1 2 3 4 5; do mv "$dir/out/seg-$t-$i.mkv" "$dir/cat/"; echo "file 'seg-$t-$i.mkv'" >> "$dir/cat/list.txt"; done
"$here/ffd.sh" "$dir/cat" -f concat -safe 0 -i list.txt -vf "tpad=stop_mode=clone:stop_duration=0.6" -an \
  -c:v libx264 -preset slow -crf "$crf" -pix_fmt yuv420p -movflags +faststart "out/film-$t.mp4"
for i in 0 1 2 3 4 5; do printf '%s ' "$(docker run -i --rm --entrypoint ffprobe linuxserver/ffmpeg:latest -v error -count_frames -show_entries stream=nb_read_frames -of csv=p=0 -i pipe:0 < "$dir/cat/seg-$t-$i.mkv")"; done; echo
