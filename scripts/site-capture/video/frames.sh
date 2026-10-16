#!/usr/bin/env bash
# frames.sh <in> <out.png> <cols> <width> t1 t2 ... : a grid of frames at these seconds
in=$1 out=$2 cols=$3 w=$4; shift 4
sel=$(for t in "$@"; do printf "lt(prev_pts*TB\\,%s)*gte(pts*TB\\,%s)+" "$t" "$t"; done); sel=${sel%+}
n=$#; rows=$(( (n + cols - 1) / cols ))
"$(dirname "$0")/../ffm.sh" "$in" "$out" -vf "select='$sel',scale=$w:-1,tile=${cols}x${rows}" -frames:v 1 -fps_mode vfr 2>&1 | grep -v -i fontconfig | grep -v '^\s*/' | grep -v '^$'
