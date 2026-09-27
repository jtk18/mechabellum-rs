# mechabellum-rs

Replay parsing, stats, and automated testing for [Mechabellum](https://store.steampowered.com/app/669330/Mechabellum/).

| Path | What |
|---|---|
| `crates/mechabellum` | Replay parser library (`mechabellum`) and CLI (`replay-parse`). Parses every replay version from 1425 onward and resolves unit/tech/specialist/skill/item IDs to names. |
| `crates/server` | Replay ingestion and stats API (axum + SQLite). Runs standalone or embedded in the desktop app. |
| `crates/tester` | Game UI automation: launches the game, OCR, and a YAML test DSL. See its `CLAUDE.md`. Windows only. |
| `apps/desktop` | Tauri app that will sit in the background, watch the replay folder, and show stats. Builds with its own `npm run tauri` tooling. |

## Parser

```sh
# Summarize a replay or a directory of replays (.grbr or extracted .xml)
cargo run -p mechabellum --bin replay-parse -- "<Steam>/steamapps/common/Mechabellum/ProjectDatas/Replay"

# Regenerate the bundled ID -> name table after a game patch
cargo run -p mechabellum --bin replay-parse -- gen-names --game-dir "<Steam>/steamapps/common/Mechabellum"
```

`--delete-original` replaces each `.grbr` with its extracted `.xml`. Don't point it at the game's
replay folder unless you have another copy of those replays.

Names come from the game's own localization table (`Mechabellum_Data/resources.assets`), written to
`crates/mechabellum/data/names.json`. The hand-written enums keep covering content that has since
been removed from the game, so older replays still resolve.

## Server

```sh
# Backfill from a replay folder (reads only), then serve
cargo run --release -p mechabellum-server -- --db mechabellum.db import "<Steam>/steamapps/common/Mechabellum/ProjectDatas/Replay"
cargo run --release -p mechabellum-server -- --db mechabellum.db serve --bind 127.0.0.1:8080
```

| Endpoint | |
|---|---|
| `POST /api/replays` | Body is a raw `.grbr` or `.xml`. Returns 201 when added, 200 when already present, 400 when the body isn't a replay, and 422 when it's a replay that can't be parsed. With `--require-auth`, it requires `Authorization: Bearer <key>`. |
| `GET /api/replays?limit=50` | Most recent matches. |
| `GET /api/stats` | Unit and specialist pick and win rates, taken from each player's final-round army. Filters: `min_version`, `max_version`, `match_mode` (`VS_1_1`, ...), `game_mode` (`Normal`, `AsyncBattle`, ...), `player_id`. |
| `GET /health` | |

The database stores each original replay (gzipped). After upgrading the parser, run
`mechabellum-server reindex` to rebuild every stat from those stored replays.

### Upload auth

The local instance (the desktop app) accepts uploads from anyone. A public instance runs with `--require-auth`
(`MECHABELLUM_REQUIRE_AUTH=true`) and accepts either of these:

- **Upload keys**, one per client. Create them with `keys add <name>`. Only the key's SHA-256 is stored, and the key itself is printed once. `keys revoke <name>` cuts a client off immediately, and each match records which key uploaded it.
- An optional **admin token** (`MECHABELLUM_UPLOAD_TOKEN`), compared in constant time.

The server refuses to start in auth mode if neither exists. Later, when "sign in with X" accounts arrive, keys get tied to
accounts. The provider's own tokens only prove who someone is when they sign in on the website. They are never used to upload.

### Consistency

All writes go through one SQLite connection per process and run in `BEGIN IMMEDIATE` transactions, with a busy timeout.
Uploads therefore apply one at a time, even when `import` runs next to the server. Each replay is stored whole or not at all,
and `battle_id` is the primary key, so a replay can't be stored twice. Parsing and compression happen before taking the lock.

### Deploying (Ubuntu + Docker)

```sh
docker compose build
docker compose run --rm server keys add jason-desktop   # prints the key once
docker compose up -d
# optional backfill: docker compose run --rm -v /path/to/replays:/replays:ro server import /replays
```

The container listens on `127.0.0.1:8080`. Put the reverse proxy (Pingora) in front of it for TLS.
