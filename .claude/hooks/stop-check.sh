#!/usr/bin/env bash
# Puerta de calidad al terminar. Solo corre si hubo cambios desde la última pasada:
#   src/ → pnpm check (svelte-check) · src-tauri/src → cargo check
# Si algo falla, exit 2 devuelve el error a Claude.
set -u
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
stamp=".claude/.last-check"
[ -f "$stamp" ] || touch -t 197001010000 "$stamp"

changed() { [ -n "$(find "$@" -type f -newer "$stamp" -print -quit 2>/dev/null)" ]; }

fail=0
if changed src; then
  if ! out=$(pnpm -s check 2>&1); then
    echo "pnpm check falló — corrígelo antes de terminar:" >&2
    echo "$out" | tail -40 >&2
    fail=1
  fi
fi
if changed src-tauri/src src-tauri/Cargo.toml; then
  if ! out=$(cd src-tauri && cargo check --quiet --all-features 2>&1); then
    echo "cargo check falló — corrígelo antes de terminar:" >&2
    echo "$out" | tail -40 >&2
    fail=1
  fi
fi

[ "$fail" -eq 0 ] && touch "$stamp" && exit 0
exit 2
