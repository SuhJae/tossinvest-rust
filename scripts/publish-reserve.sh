#!/usr/bin/env bash
#
# Publish the `tossinvest` crate family to crates.io in DEPENDENCY ORDER.
#
# Prerequisites:
#   1. A verified crates.io account (https://crates.io, "Log in with GitHub").
#   2. `cargo login <token>` with a token scoped to `publish-new` + `publish-update`.
#
# NOTE: publishing is PERMANENT — a version can be yanked but never deleted.
#       These 0.0.0 crates reserve the names; real code ships as 0.1.0+.
#
# Usage:  bash scripts/publish-reserve.sh           # publish for real
#         DRY_RUN=1 bash scripts/publish-reserve.sh  # package + verify only
set -euo pipefail

cd "$(dirname "$0")/.."

FLAG=""
[ "${DRY_RUN:-0}" = "1" ] && FLAG="--dry-run"

publish() {
  local dir="crates/$1"
  echo "==> publishing $1 $FLAG"
  ( cd "$dir" && cargo publish $FLAG )
  if [ -z "$FLAG" ]; then
    echo "    waiting ~15s for index propagation before the next crate..."
    sleep 15
  fi
}

# Order matters: dependents must come after their dependencies.
publish tossinvest-model   # no internal deps
publish tossinvest-rate    # no internal deps
publish tossinvest         # depends on model + rate
publish tossinvest-state   # depends on model + rate + tossinvest

echo
echo "Done. View at https://crates.io/crates/tossinvest"
