#!/usr/bin/env bash
# Checks prerequisites, then builds the frontend and the backend.
# Usage: ./install.sh [-y]    (-y installs missing system packages without asking)
set -euo pipefail

cd "$(dirname "$0")"
assume_yes=false
[[ "${1:-}" == "-y" ]] && assume_yes=true

info() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# rustup installs here without necessarily touching the current shell's PATH.
[[ -d "$HOME/.cargo/bin" ]] && PATH="$HOME/.cargo/bin:$PATH"

# A tool counts as working only if it actually runs, not merely if it is on the PATH.
works() { "$@" >/dev/null 2>&1; }

missing=()
works cargo --version || missing+=(rust)
works node --version && works npm --version || missing+=(node)
works "${STOCKFISH_PATH:-stockfish}" --help || missing+=(stockfish)

if ((${#missing[@]})); then
  info "Missing or broken: ${missing[*]}"
  if command -v pacman >/dev/null; then
    declare -A pkg=([rust]=rust [node]="nodejs npm" [stockfish]=stockfish)
    # -Syu rather than -S: a partial upgrade is what leaves node linked against a library that is gone.
    install=(sudo pacman -Syu --needed)
  elif command -v apt-get >/dev/null; then
    declare -A pkg=([rust]=cargo [node]="nodejs npm" [stockfish]=stockfish)
    install=(sudo apt-get install -y)
  elif command -v dnf >/dev/null; then
    declare -A pkg=([rust]=cargo [node]="nodejs npm" [stockfish]=stockfish)
    install=(sudo dnf install -y)
  else
    fail "no supported package manager found; install ${missing[*]} manually and re-run"
  fi

  packages=()
  for tool in "${missing[@]}"; do
    # shellcheck disable=SC2206
    packages+=(${pkg[$tool]})
  done
  command="${install[*]} ${packages[*]}"
  if ! $assume_yes; then
    read -rp "Run '$command'? [y/N] " answer
    [[ "$answer" =~ ^[Yy]$ ]] || fail "install ${missing[*]} and re-run"
  fi
  $command

  works cargo --version || fail "cargo still does not run"
  works node --version || fail "node still does not run (try a full system upgrade)"
  works "${STOCKFISH_PATH:-stockfish}" --help || fail "stockfish still does not run"
fi

info "Building frontend"
(cd frontend && npm ci && npm run build)

info "Building backend"
(cd backend && cargo build --release)

info "Done. Start the server with:"
echo "    cd $(pwd)/backend && ./target/release/chess-analyzer"
echo "then open http://127.0.0.1:3001"
