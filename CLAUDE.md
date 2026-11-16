# CLAUDE.md

Guidance for Claude Code in this repository. Read `ARCHITECTURE.md` first; `README.md` has the
endpoints and the deployment-class rules, `docs/review-2026-10-27.md` the decisions so far and
what's still open, `docs/TODO.md` the backlog. `crates/tester/CLAUDE.md` covers that crate only.

## Commands

```sh
# The local gate before any push: the whole workspace, not only the crates touched
(cd apps/desktop && npx --no-install vite build)   # the desktop crate embeds dist/
cargo test --workspace --exclude mechabellum-tester  # the tester needs Windows
cargo clippy --workspace --all-targets --exclude mechabellum-tester
cargo fmt --check -p mechabellum -p mechabellum-server   # see "Formatting" below
(cd apps/desktop && npx --no-install tsc --noEmit -p .)

# The rebuild control: replays every round that has a next record and compares
MECHABELLUM_REPLAYS=<dir of .xml replays> cargo test -p mechabellum -- --ignored replay_folder --nocapture

# A disposable server on a copy of a database
cargo run -p mechabellum-server -- --db /tmp/copy.db serve --bind 127.0.0.1:18080

# The desktop app (embeds the server on 127.0.0.1:8765)
(cd apps/desktop && npx --no-install tauri build --debug --no-bundle)
./target/debug/mechabellum-desktop
```

`cargo test` doesn't rebuild the server binary; run `cargo build -p mechabellum-server` before
using it. The website pages' inline scripts have no build step: check them with `node --check`
on the extracted `<script>` body.

## Rules

- **CI.** Push to `main` with `[skip ci]` in the commit message, after the local gate passes.
  Don't start workflow runs, and never push a tag: a `v*` tag starts the release workflow.
- **Formatting.** Don't run `cargo fmt` over the workspace. `crates/mechabellum/src/board.rs` and
  `crates/grounds` carry the owner's formatting; reformatting them buries real changes. Format the
  files you change: `rustfmt --edition 2021 <file>`.
- **Privacy.** The repository is public. Nothing committed
  names another player: not fixtures, docs, comments or commit messages. Refer to a match by its
  battle ID. Don't add fixtures holding other players' names.
- **Comments.** A comment says what the code does and why. No quotes from chat, no attributions
  to people.
- **Schema.** Any change to what the derived tables hold bumps `SCHEMA_VERSION` in
  `crates/server/src/store.rs`, with a one-line note above it. The next open rebuilds from the
  stored replays; a new column on a base table also needs an `ALTER TABLE` in `Store::init`.
- **Rules and definitions** (deployment bands and classes, what a stat counts) are the owner's
  calls. Their constants sit at the top of `summary.rs`; the README, `deploy.ts` (`RULES`),
  `match.ts`, `web/matches.html` and `web/stats.html` restate them and change with them.
- **Untrusted input.** Replays arrive from uploads. Bound every number read from one before doing
  arithmetic on it (`detail.rs`, `final_round.rs` show the pattern); the server contains a parser
  panic per replay, but release builds wrap overflows silently.
- **Controls.** A measurement carries a control that could have failed: the rebuild control above,
  results against reactor HP, an independent recount. Give its scope (which replays, how many).
- **The local app.** After a parser or server change, rebuild the desktop app and relaunch it;
  `cargo check` isn't enough. The app's database lives in the OS app-data folder under
  `com.jtk18.mechabellum-stats`; work on a copy of it, never the live file.
