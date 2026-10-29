#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
[ "$#" -ge 1 ] || { echo "Usage: scripts/test-vm.sh PRIVATE_ABSOLUTE_WORKDIR [run.py options]" >&2; exit 2; }
work=$1; shift
exec timeout --signal=TERM --kill-after=90s 1800s python3 "$root/tests/vm/run.py" --workdir "$work" "$@"
