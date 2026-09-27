# mechabellum-rs

Replay parsing, stats, and automated testing for [Mechabellum](https://store.steampowered.com/app/669330/Mechabellum/).

| Path | What |
|---|---|
| `crates/mechabellum` | Replay parser library (`mechabellum`) and CLI (`replay-parse`). Parses every replay version from 1425 onward and resolves unit/tech/specialist/skill/item IDs to names. |
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
