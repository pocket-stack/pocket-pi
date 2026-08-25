#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
simulator_root="${POCKETPI_SIM_ROOT:-$repo_root}"
simulator_bin="${POCKETPI_SIM_BIN:-$simulator_root/target/debug/pocket-pi-esp32-sim}"
site_root="$(cd "$(dirname "$0")/.." && pwd)"
screen_output="$site_root/public/pocketpi-device/screens"
simulator_workspace="$(mktemp -d "${TMPDIR:-/tmp}/pocketpi-s3-hero.XXXXXX")"
trap 'rm -rf "$simulator_workspace"' EXIT

mkdir -p "$screen_output"

"$simulator_bin" --screenshot "$screen_output/main.png" \
  --viewport 480x800 --workspace "$simulator_workspace" --backend codex
"$simulator_bin" --screenshot "$screen_output/files.png" \
  --viewport 480x800 --workspace "$simulator_workspace" --backend codex --tap 180,760
"$simulator_bin" --screenshot "$screen_output/apps.png" \
  --viewport 480x800 --workspace "$simulator_workspace" --backend codex --tap 300,760
"$simulator_bin" --screenshot "$screen_output/settings.png" \
  --viewport 480x800 --workspace "$simulator_workspace" --backend codex --tap 420,760
