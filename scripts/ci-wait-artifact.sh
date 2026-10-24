#!/usr/bin/env bash
# Usage: ci-wait-artifact.sh <artifact> <job>
# Waits until the job <job> of this workflow run has uploaded <artifact>, so the calling job can
# do its own setup while <job> builds. Fails as soon as <job> ends without it. Needs GH_TOKEN with
# actions: read, and GITHUB_REPOSITORY and GITHUB_RUN_ID (set in every Actions job).
set -euo pipefail

artifact="$1"
job="$2"
run="repos/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID"

uploaded() {
  [ "$(gh api "$run/artifacts?name=$artifact" --jq '.total_count')" -gt 0 ]
}

# One look every 10 s keeps the calls of all waiting jobs far below the token's hourly limit.
for _ in $(seq 180); do
  if uploaded; then
    exit 0
  fi
  state="$(gh api "$run/jobs?filter=latest&per_page=100" \
    --jq ".jobs[] | select(.name == \"$job\") | .status + \" \" + (.conclusion // \"\")")"
  if [ "${state%% *}" = completed ]; then
    # The upload is the job's last step: look once more before giving up.
    uploaded && exit 0
    echo "$job ended ($state) without the artifact $artifact." >&2
    exit 1
  fi
  sleep 10
done
echo "$job gave no $artifact in 30 minutes." >&2
exit 1
