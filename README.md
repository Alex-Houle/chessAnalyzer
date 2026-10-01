# Chess Game Review

Pulls a player's games from the Chess.com public API and reviews them with the
Stockfish binary installed on the server: per-move classification (best → blunder),
missed mates and missed tactics, accuracy, and an evaluation graph.

- `backend/` — Rust (axum + tokio). Talks to Chess.com, drives Stockfish over UCI.
- `frontend/` — React + Tailwind (Vite).

## Requirements

- Rust (stable), Node.js 20+
- `stockfish` on the `PATH`, or `STOCKFISH_PATH` pointing at the binary

## Install

```sh
./install.sh
```

Checks that Rust, Node.js and Stockfish actually run (offering to install any that are
missing via pacman, apt or dnf), then builds the frontend and the backend.

## Run

Production-style (the backend serves the built frontend):

```sh
cd frontend && npm install && npm run build
cd ../backend && cargo run --release
# → http://127.0.0.1:3001
```

Development with hot reload (Vite proxies `/api` to the backend):

```sh
cd backend && cargo run          # terminal 1
cd frontend && npm run dev       # terminal 2 → http://localhost:5173
```

## Configuration (environment variables)

| Variable | Default | Purpose |
|---|---|---|
| `HOST` / `PORT` | `127.0.0.1` / `3001` | Listen address. Use `HOST=0.0.0.0` to expose it. |
| `STOCKFISH_PATH` | `stockfish` | Engine binary. |
| `STOCKFISH_WORKERS` | min(cores, 8) | Engine processes per analysis; each searches different positions of the game. |
| `STOCKFISH_THREADS` | `1` | Threads per engine process. |
| `STOCKFISH_HASH_MB` | `64` | Hash size per engine process. |
| `MAX_CONCURRENT_ANALYSES` | `1` | Games analyzed at once; extra requests queue. |
| `CHESSCOM_USER_AGENT` | `chess-analyzer/<version>` | Chess.com asks for a contact address here, e.g. `myapp (me@example.com)`. |
| `STATIC_DIR` | `../frontend/dist` | Built frontend to serve. |

## API

| Route | Description |
|---|---|
| `GET /api/players/{username}/archives` | Months with games, newest first. |
| `GET /api/players/{username}/games/{year}/{month}` | Standard-chess games for that month. |
| `POST /api/analysis` `{pgn, depth?}` | Starts (or reuses) an analysis job → `{id}`. Depth 6–24, default 14. |
| `GET /api/analysis/{id}` | `queued` / `running` (both with `done`, `total` and the partial `result` so far) / `done {result}` / `error {message}`. |
| `DELETE /api/analysis/{id}` | Cancels an unfinished analysis and kills its engines. |

## How moves are judged

Every position is searched to a fixed depth and its score converted to winning
chances (Lichess' logistic model). A move is judged by the winning chances it gives up:

| Loss (percentage points) | Class |
|---|---|
| engine's first choice | Best |
| < 2 | Excellent |
ls | < 5 | Good |
| < 10 | Inaccuracy |
| < 20 | Mistake |
| ≥ 20 | Blunder |

A move is flagged as a **missed tactic** when the player had a forced mate and let it
go, or when the opponent had just made a mistake/blunder and the reply gave up ≥ 10
points instead of punishing it.

A game's positions are split across a pool of single-threaded engines, and the client
polls for partial results, so moves are reviewed on screen as they come in. Leaving a
game or changing the depth cancels its analysis.

Results are cached in memory (last 200 analyses) and lost on restart.

## Credits

Chess pieces in `frontend/public/pieces/` are the "cburnett" set by Colin M.L. Burnett,
as distributed by Lichess (licensed GPLv2+).
