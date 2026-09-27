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
| `POST /api/replays` | Body is a raw `.grbr` or `.xml`. Returns 201 when added, 200 when already present. Requires `Authorization: Bearer <token>` when `MECHABELLUM_UPLOAD_TOKEN` is set. |
| `GET /api/replays?limit=50` | Most recent matches. |
| `GET /api/stats` | Unit and specialist pick and win rates, taken from each player's final-round army. Filters: `min_version`, `max_version`, `match_mode` (`VS_1_1`, ...), `game_mode` (`Normal`, `AsyncBattle`, ...), `player_id`. |
| `GET /health` | |

The database stores each original replay (gzipped). After upgrading the parser, run
`mechabellum-server reindex` to rebuild every stat from those stored replays.

### Deploying (Ubuntu + Docker)

```sh
echo "MECHABELLUM_UPLOAD_TOKEN=$(openssl rand -hex 32)" > .env
docker compose up -d --build
# optional backfill: docker compose run --rm -v /path/to/replays:/replays:ro server import /replays
```

The container listens on `127.0.0.1:8080`. Put the reverse proxy (Pingora) in front of it for TLS.
