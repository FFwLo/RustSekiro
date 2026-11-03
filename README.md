# RSekiro

SEKIRO: SHADOWS DIE TWICE's combat rebuilt in Rust on [Bevy](https://bevyengine.org), from the game's own
files (animations, params, AI scripts, models, sounds) and checked against the game's code in an emulator.

**[Open the work map](https://claude.ai/artifact/ReFjjAWUHZ8syNQPnJBKuu)**: what is proven to match the game,
system by system and boss by boss, and which rows are open.

A fan project for study, not affiliated with FromSoftware or Activision. No game files are in this repository:
you need your own copy of the game.

## What it does

Wolf's moves, the grappling hook and the healing gourd; guard, deflect, posture, Mikiri, combat arts, prosthetic
tools and deathblows; all 102 enemy kinds running their own AI scripts, with stealth; every main boss from
Genichiro to the Divine Dragon. Values the files don't hold are marked `gap`. Not included yet: story, NPCs,
saves, the open world.

## Run it

You need Windows, Sekiro from Steam (patch 1.06), Rust and Python 3.

```powershell
powershell -ExecutionPolicy Bypass -File tools\extract.ps1   # builds extracted/ from your game, once
cargo run --release
```

Keys: [docs/controls.md](docs/controls.md). Tests: `cargo test --release`.

## Fine-tune it

The few values Sekiro's files don't hold are in `config.toml`, each with a note on where the real value lives.
Edit one while the game runs and press `F5`: no rebuild. `F1` opens the debug menu (enemy, AI, skills, cheats).

## Join

1. Fork, clone, and add `upstream`: `git remote add upstream https://github.com/FFwLo/RustSekiro.git`.
2. Extract your data, run `git config core.hooksPath .githooks`, then `python tools/doctor.py` until it says `ready`.
3. Pick an open row (`git show upstream/agents-work-board:board.py | python - open`; new here: a `B` row) and
   claim it on the [claims issue](https://github.com/FFwLo/RustSekiro/issues/3).
4. Open a pull request with the full suite's result. The owner reviews and merges.

Everything else, including the emulator setup and the Claude Code skill, is in
[docs/CONTRIBUTING.md](docs/CONTRIBUTING.md). No game files, extracted data or decompiled code go into the
repository; function addresses are fine.

## License

Code under the [MIT License](LICENSE.md); it covers this code only, not the game or anything generated from it.
The crates in `crates/` keep sekiro-rs's MIT / Apache-2.0 licence ([credits](crates/CREDITS-sekiro-rs.md)).
