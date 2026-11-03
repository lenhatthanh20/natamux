#!/usr/bin/env bash
# Run on the Mac desktop session, with no package installs or app data changes.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
exec python3 "$here/native-record.py" "$@"
