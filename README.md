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
| `GET /api/stats/games` | Whole matches rather than final armies: win rates by round 1 deployment (and round 1 deployment against the opponent's, in 1v1), how often both players went asym and on which lanes, match lengths, winners' and losers' mean army size and value, levels and units sold, and every unit fielded at any point. Same filters. Plain rates, not capped per uploader. |
| `GET /api/cards?limit=24&offset=0` | Match cards, newest first: each player's whole game (`mechabellum::PlayerGame`: army size and levels, units used and sold, every fight's result, HP, deployment and board). Same filters. Names as stored, never account IDs (and on a public server, see Pseudonyms). |
| `GET /health` | |

Pages: `/` (landing), `/stats`, and `/matches` (one card per match). The last two aren't linked from the
landing page yet.

**Deployment classes** (`mechabellum::classify`, per player per fight, 1v1 on the 2.0 board from build
2259): each unit falls in a band by where it stands in its own half. **Aggressive**: front edge within 5
squares of the zone's front edge, or in a flank column. **Midboard**: from there back to the towers' centers.
**Backboard**: behind the towers' centers. Units weigh by footprint area; the army takes the class of the band
holding the most of it when that's aggressive or backboard with at least 40%, and is midboard otherwise. A
midboard army is **standard** when its ground is spread across both sides and **asym mid aggro** when three
quarters or more of it stands on one side (judged on the whole army, since midboard is also the class of an army
no band leads). An aggressive army is **sym** or **asym** by its aggressive ground the same way; backboard isn't
split yet. When both players in a 1v1 are
asym, they meet in an **asym headbutt** on the same lane, or an **asym crossfire** on opposite lanes
(`mechabellum::meeting`). A match is
classed by each player's **round 1** layout, not the last round's; a replay with no round 1 record isn't
classed. Later fights are classed the same way, which the
match cards' fight tape shows.

The database stores each original replay (gzipped). After upgrading the parser, run
`mechabellum-server reindex` to rebuild every stat from those stored replays.

### Upload auth

The local instance (the desktop app) accepts uploads from anyone. A public instance runs with `--require-auth`
(`MECHABELLUM_REQUIRE_AUTH=true`) and accepts either of these:

- **Upload keys**, one per client. Create them with `keys add <name>`. Only the key's SHA-256 is stored, and the key itself is printed once. `keys revoke <name>` cuts a client off immediately, and each match records which key uploaded it.
- An optional **admin token** (`MECHABELLUM_UPLOAD_TOKEN`), compared in constant time.

The server refuses to start in auth mode if neither exists. Later, when "sign in with X" accounts arrive, keys get tied to
accounts. The provider's own tokens only prove who someone is when they sign in on the website. They are never used to upload.

### Pseudonyms

A server run with `--require-auth` stores no account IDs and no names. Every replay is rewritten as it's stored: each
player's account ID becomes an HMAC of it under a secret key (`p` and 20 hex digits), and every name becomes `Player`.
A plain hash wouldn't do: names are public and account IDs count up from a fixed prefix, so either can be hashed and
compared by anyone. The keyed ID is stable across renames and uploaders, so key binding, per-uploader caps and
corroboration work as before.

- The key is `pseudonym.key` beside the database (`--pseudonym-key-file` to move it), created the first time
  `serve --require-auth` runs. **Back it up with the database.** The database records a check value; once it's
  pseudonymized, every command needs the same key and refuses to run without it, so nothing raw is stored again.
- Turning it on for a database that already holds replays rewrites them, the tables that name accounts (ratings, key
  bindings, copies), and rebuilds the stats, once.
- Responses carry nothing that ties one match to a player's others: players are numbered within each match, ratings
  are rounded to 100, tech loadouts are left out, and per-player filters (`player_id`, `opponents_of`) are refused.
  The desktop app's own server is unchanged.
- Each Mechabellum account can be shared for by one uploader at a time; a second key claiming it gets 409.

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

## Desktop releases and updates

`.github/workflows/release.yml` builds the installers. Push a tag matching the version in
`apps/desktop/src-tauri/tauri.conf.json` (`v0.2.0` for `0.2.0`) and it makes a draft release
with the Windows, macOS (universal) and Linux bundles, each updater artifact's signature, and a
`latest.json` that lists them.

Installed copies update themselves from that file:

- The app asks `https://github.com/jtk18/mechabellum-rs/releases/latest/download/latest.json`
  shortly after it starts and every four hours, unless "Check for updates automatically" is off
  in Settings; "Check now" there checks at any time. A newer version shows a banner with its
  version and notes and an "Install and restart" button.
- Installing downloads the bundle, checks its signature against the public key in
  `tauri.conf.json` (`plugins.updater.pubkey`), and refuses anything else. macOS replaces the
  `.app` and restarts; Windows runs the installer in passive mode, which restarts the app.
- `releases/latest` skips drafts and prereleases: nothing is offered until the draft is
  published, and manual workflow runs (prereleases) never are. While the repository is private
  the address answers 404, and Settings says no published release was found. Failures are
  logged and shown in Settings, never as a dialog.
- A build run from a cargo target folder (`target/debug/...`) never checks: installing over it
  would replace the build folder. Only installed copies update.
- Before publishing a draft, open its `latest.json` and check it lists `darwin-aarch64`,
  `darwin-x86_64`, `windows-x86_64` and `linux-x86_64`. Each bundle job adds its platforms to the
  file on the release, so two jobs finishing together can drop one; re-run that job if so. Its
  `notes` are the release body at build time (empty unless the workflow sets `releaseBody`), not
  what the draft says when it's published.

### The updater key

Every update is signed with one key pair. The public half is committed in `tauri.conf.json`;
the private half and its password live outside the repository:

- `~/.config/mechabellum-stats/updater.key` and `updater.key.password` (mode 600) on the
  maintainer's Mac;
- the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets,
  which the release workflow passes to the build. It fails at its first step if either is missing.

**Back the key and password up somewhere other than this Mac.** Installed copies only accept
updates signed by this key. If it's lost, no installed copy can ever update again: a new key
means a new public key in the app, and every player has to download and install that version by
hand. Anyone holding the key can sign builds those copies will install, so keep it private.

A local bundled build (`npx --no-install tauri build` without `--no-bundle`) signs the updater
artifacts too and needs the key in the environment:

```sh
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.config/mechabellum-stats/updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat ~/.config/mechabellum-stats/updater.key.password)"
```
