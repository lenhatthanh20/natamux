#!/bin/sh
# Edge cases of shell colours.
export PATH="$HOME/bin:$PATH"
declare -a items=(one "two words" 'three')
declare -A prices=([one]=1 [two]=2)

report() {
  local total=0
  for item in "${items[@]}"; do
    (( total += ${#item} ))
  done
  echo "total: $total, first: ${items[0]:-none}, count: ${#items[@]}"
}

cat <<EOT > /tmp/report.txt
Report of $(date +%F)
  value: ${prices[one]}
EOT

if [ -n "${1-}" ] && test "$1" != "--quiet"; then
  report | sed -e 's/total/sum/' 2>/dev/null || true
elif [[ $1 =~ ^-[a-z]$ ]]; then
  diff <(sort a.txt) <(sort b.txt)
fi
while read -r line; do echo "$line"; done < /tmp/report.txt
exit $?
