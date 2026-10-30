# Architecture

Mechabellum replays in, stats out. The game saves every match as an XML battle record (`.grbr`
files hold the same XML). A Rust parser turns a record into a per-match model; a server stores
the records in SQLite, derives tables from them and serves an API and two web pages; a desktop
app embeds that same server, watches the game's replay folder and shows the stats in its own UI.

## Workspace

| Path | Crate | What it is |
|---|---|---|
| `crates/mechabellum` | `mechabellum` | The replay parser and game data: the XML model, the match model, deployment classes, validation, names, board geometry. Also the `replay-parse` CLI. |
| `crates/server` | `mechabellum-server` | axum + SQLite (rusqlite, bundled). Ingest, schema and rebuilds, every stats query, the HTTP API, and the website pages in `web/`. CLI: `serve`, `import`, `reindex`, `purge`, `disputes`, `outliers`, `keys`. |
| `apps/desktop` | `mechabellum-desktop` | Tauri 2 app. Rust side in `src-tauri/`, TypeScript frontend in `src/` (built with Vite into `dist/`, which the binary embeds). |
| `crates/grounds` | `mechabellum-grounds` | Sets up a board in the game's Testing Grounds from a board code (Windows; drives the game). |
| `crates/tester` | `mechabellum-tester` | Windows UI automation for the game (OCR, template matching). Has its own `CLAUDE.md`. |

The `Dockerfile` builds the server alone for hosting.

## Data flow

```
replay XML ──► BattleRecord (serde) ──► MatchDetail ──► PlayerGame per player
                    │                       │                 │
                    │ validate() (shared    │ overview        │ summary.rs
                    │ uploads only)         │ (final army)    │
                    ▼                       ▼                 ▼
server: replay_blobs (gzip)   matches, players, player_units, player_games, player_fights,
                              player_techs, player_cards      player_unit_use  (derived)
                                        │
                     SCOPE / MATCH_SCOPE filters ──► /api/stats, /api/stats/games,
                                                     /api/cards, /api/replays, ...
                                        │
                  website (web/stats.html, web/matches.html)   desktop UI (Tauri commands)
```

Every stored replay is kept as gzipped XML, so any table can be rebuilt from scratch.

## The parser (`crates/mechabellum/src/replays/`)

- **`battle_record.rs`**: the raw XML model (`BattleRecord`, `PlayerRecord`, round records,
  actions). Results live here: `match_results()` reads version 2340's per-fight
  `roundFightResult` list; before 2340 the result is the next-to-last fight's
  (`preRoundFightResult` on the last record), and needs the round before the last recorded. A
  concession during deployment (`PAD_GiveUp` before `FinishDeploy`) ends the match a fight early.
  `results_settled()` says whether the replay names as many winners as the mode has.
- **The round model.** Player round record *n* holds the army as round *n*'s deployment began,
  plus that round's actions. The army that fought fight *n* is therefore record *n + 1*'s. The
  last round has no next record, so `final_round.rs` rebuilds it by replaying its actions
  (`replay_deployment`). Unit cards, unit-granting specialists and summons take unit indices the
  replay doesn't count; `align_claims` chooses each one's count to agree with the round's
  `MoveUnit` facts. Control: every 2.0 round that has a next record rebuilds exactly.
- **`detail.rs`**: `MatchDetail`, the per-fight layout of each side (`SideLayout`, `PlacedUnit`
  with level, XP gained, value in supply, placement), cards offered and chosen, towers, techs,
  reactor HP. Unit value is the unit's `SellSupply`, with fallbacks for last-round purchases.
- **`summary.rs`**: `PlayerGame`, a whole match from one side: army size and value, levels,
  level-ups, units added and sold, techs, cards, units used, and each fight's deployment class
  (`classify`: aggressive / midboard / backboard bands, sym or asym by lean), plus how two 1v1
  deployments meet (asym headbutt or crossfire). The rules are written out in the README.
- **`overview.rs`**: the older per-player summary (final army, specialists, techs, cards) that
  the final-army pick and win rates use.
- **`validate.rs`**: problems that make a shared upload implausible (results that contradict
  reactor HP, impossible armies, wrong IDs). Runs on shared uploads only.
- **`game_data.rs`**: facts inferred from replays: unit footprints, XP thresholds, list prices.
  **`names.rs`** + `data/names.json`: game IDs to English names, extracted from the game.
- **`board.rs`** (crate root): board geometry and board codes (`docs/board-code.md`).

Coordinates are replay units, 10 per grid square, in a shared frame with the south half at
negative y; each side's own frame mirrors the north (negate x and y).

## The server (`crates/server/src/`)

- **`store.rs`**: the SQLite schema, ingest and rebuilds.
  - Base tables (`matches`, `players`, `player_ratings`, `player_units`, `player_techs`,
    `player_cards`, `replay_blobs`, upload keys, match copies) and derived tables
    (`player_games`, `player_fights`, `player_unit_use`).
  - `SCHEMA_VERSION`: bumping it makes the next open drop the derived tables and `reindex`
    every stored replay. Each bump has a one-line note above the constant.
  - `reindex` streams: a reader thread pages through `replay_blobs`, parsers in the caller's
    rayon pool (the desktop app's is low-priority) take them over a bounded channel, and a
    writer thread fills a scratch database beside the real one (`<db>.rebuild-<pid>-<n>`). Replays
    stored meanwhile get more unlocked rounds; the swap attaches the scratch file and copies its
    rows over in one transaction. Memory stays near 100 MB however many replays are stored
    (7,552 measured), where holding them all took about 1 MB each.
  - `ingest` (local and admin) stores a replay as one transaction; `ingest_shared` (community
    keys) validates first, hides other players' names, and needs the uploader in the match.
  - Parser work on one replay runs inside `contained()`, so a panic costs that replay, never a
    rebuild or a request.
  - `SCOPE` is the set of player results every stat counts: the filters (versions, mode, player,
    opponents), corroborated-only, offline games, settled matches, and matches the logged-in
    account played (`played_here!`). `MATCH_SCOPE` is the same without the settled condition, for
    the match lists.
- **`games.rs`**: match cards and the whole-match stats (`/api/cards`, `/api/stats/games`).
- **`aggregate.rs`**: stats no single uploader can move much: each uploader's weight capped, a
  median of per-uploader win rates, and uploaders far from everyone else flagged
  (`docs/upload-trust.md`).
- **`lib.rs`**: the router, upload auth (keys, admin token), size caps on bodies and gzip, the
  pages, and the tests.
- **`web/`**: static pages with inline JS, compiled into the binary: `index.html` (landing),
  `stats.html`, `matches.html`.

## The desktop app (`apps/desktop/`)

- **`src-tauri/src/lib.rs`**: opens the database in the app data folder, upgrades it in the
  background, serves the embedded router on `127.0.0.1:8765`, and exposes Tauri commands that
  call the `Store` directly.
- **`watcher.rs`**: finds the replay folder and imports new replays. **`gamelog.rs`**: reads the
  game's `Player.log` for MMR and the logged-in account (`local_player`). **`sync.rs`**: shares
  replays with a community server, other players' names removed first. **`steam.rs`**: Steam and
  install lookups, re-exported from the parser crate's `install.rs`. **`art.rs`**: unit art read
  from the player's own install (`unity-bundle-assets`); none of it ships or is committed.
  **`updates.rs`**: checks the GitHub releases' `latest.json` through Tauri's updater plugin,
  holds a found update for the banner, installs it after its signature checks out, and restarts.
  Builds run from a cargo target folder never check (README, "Desktop releases and updates").
- **Frontend**: `main.ts` (Stats view, match list, filters), `match.ts` (the board view with the
  fight tape and tale of the tape), `deploy.ts` (deployment classes, legends, the tale),
  `loadout.ts`, `sharing.ts`, `updates.ts` (the update banner and Settings panel), `art.ts`,
  `util.ts`, `styles.css`.

The website and the desktop UI draw the same API data separately. Constants they share with the
parser (band edges, class labels, rule text) are duplicated in `deploy.ts`, `match.ts`,
`web/matches.html` and `web/stats.html`; change them together with `summary.rs`.

## Docs

- `README.md`: running things, endpoints, upload auth, deployment classes.
- `docs/review-2026-10-27.md`: the review rounds, what they fixed, decisions still open, the
  proposed stats.
- `docs/TODO.md`: the backlog. `docs/upload-trust.md`: what a shared upload must pass.
- `docs/board-code.md`, `docs/testing-grounds.md`: board codes and the Testing Grounds tool.
