# Work list

Last updated 2026-09-27. Newest decisions are in `ORIENTATION.md` at the scratchpad root.

## Overnight loop (autonomous)

Rules for each pass: take the top unfinished item, keep the game's Replay folder read-only,
run `cargo test --workspace` and `npm run build` (apps/desktop) before committing, commit and
push each finished item, tick it here, and note surprises in ORIENTATION.md. Nothing is sent
to outside servers; sync tests use a local server on 127.0.0.1.

0. ~~Hidden window goes stale; 10 uncounted matches~~ Done: the page refreshes when shown, and the
   status line says how many matches ended before the first fight (no result to count).
   Tonight's ratings from `Player.log` are all in `player_ratings`.
Then items 1, 2, 4, 5, 6, 7, 8, 9, 11, 13, 14 (single instance, autostart), 24, 21, 12.

Needs the user (skip overnight): 15 (Mac/Linux machines), 16 (the Ubuntu server), 17 (Steam
Web API key), 22 (drives the live game), 23 (deleting GitHub branches).

## In progress: sharing matches (trust phase 1)

Done: server side (validation, key binding to one account, name hiding, `/missing`, ratings,
purge), MMR capture from `Player.log`, and the desktop sync worker (`sync.rs`, off by default).

1. ~~Sync settings UI~~ Done: a Sharing tab with server, key, on/off, progress, refused
   matches with reasons, and "Share now" (which also retries refused matches).
2. ~~End-to-end test of sync~~ Done against a local server: 1,018 of 1,025 own matches shared,
   the 7 refused are offline bot games; names hidden; ratings attached; nothing resent after a
   restart. Found and fixed on the way: the XP table (see ORIENTATION) rejected tonight's
   Centurion games, and tournament replays you watched were offered (now skipped). The
   second-key name fill is covered by `test_shared_uploads`, not re-run end to end.
3. ~~Check tonight's games~~ Done.
4. ~~Show MMR~~ Done: badges in the match list, and in the board title.

## Stats

5. ~~Win rate by opponent MMR band~~ Done: a Stats panel, opponent minus your MMR in five bands (1v1).
6. ~~Group builds~~ Done: the heatmap groups by month (default) or build. Replays carry no season, and published season dates are incomplete, so seasons are left out for now. Offline games now get their date from the battle ID.
7. ~~Tech icons by (unit, tech ID)~~ Done: `player_techs.tech_id`; 257 of 264 rows resolve by ID, the rest by name. The same name can
   have different icons per unit.
8. ~~Remaining art~~ Done: Star Expedition cards (`TD_*`), "Mass-produced" cards (item art
   `E_*_S2`), and 73 of 79 equipment icons (`E_*`, on the board's unit card and card rows). Still
   unknown: card ID 40001 (no name in the game's tables).
9. **Verify `towerStrengthenLevels`** order: index 0 is assumed to be the Command Center and
   index 1 the Research Center.

## Board

10. **Command Center and Research Center positions** on the map. Not in replays; per-map layouts
    are needed.
11. **Verify inferred footprints** against the game and correct `game_data.rs`.

## Platform and distribution

12. **Replace the AssetStudio CLI** (needs .NET 9) with native Rust extraction before
    distributing, so players don't need .NET.
13. **CI:** a GitHub Actions matrix (Windows, macOS, Ubuntu) to build and test the workspace and
    the Tauri app.
14. **Tauri bundles and installers**, start on login (optional), and single instance.
15. **Verify macOS and Linux (Proton and native)** replay and log folder detection with real
    installs.
16. **Verify the Docker build** on the Ubuntu server.

## Server and website (trust phases 2 and 3; see docs/upload-trust.md)

17. **Steam sign-in** with account-age and playtime gates, replacing bare upload keys for
    members.
18. **Corroboration:** when both players share a match, compare hashes of the shared content
    (excluding names). Add a "corroborated only" view and flag mismatches.
19. **Per-uploader caps** in aggregates; median-of-means aggregation; outlier review.
20. **Public website:** stats pages and per-player pages (players shown pseudonymously unless
    they share).

## Tester track

21. **Read unlocked techs** per unit from `ProjectDatas/GameData.grdb` instead of OCR.
22. **Automate Testing Grounds** setup from replay army snapshots (the drill-down already has
    every unit's position, level, and techs).

## Housekeeping

23. Delete the merged `version-compat` and `workspace` branches, locally and on GitHub.
24. Fix the older clippy warnings in the parser (`len() == 0`, `write!` with `\n`).
