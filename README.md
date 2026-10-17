# Sv1

A recreation of SEKIRO: SHADOWS DIE TWICE's combat, written in Rust on
[Bevy](https://bevyengine.org). Every number comes from the game's own files
(animations, params, scripts, models, sounds), not from guessing.

![Sv1 gameplay](docs/media/preview.webp)

A fan project for study, not affiliated with FromSoftware or Activision. No game
files are in this repository: you need your own copy of the game to build the
data.

## What it does

- **Wolf:** walking, running, sprinting, jumping, steps, lock-on, crouch,
  grappling hook, healing gourd, death and resurrection.
- **Combat:** attacks, guard, deflect, posture, perilous attacks, Mikiri
  Counter, combat arts, prosthetic tools with their status effects, Lightning
  Reversal.
- **Deathblows:** from the front, from behind, stealth and plunging, with the
  game's own camera angles. Bosses kneel at their last mark and the Todome
  finishes them.
- **Enemies:** all 102 kinds the game places, each running its real AI script,
  with stealth (sight, hearing, search). Every main boss with its phases, map
  events and arena, from Genichiro to the Divine Dragon.
- **Looks and sound:** the game's models, cloth, effects, blood, sky, fog,
  lighting, HUD and menus; the game's own sounds and music through FMOD.

Values the files don't hold are marked `gap` in the code and in `config.toml`.
Not included: the rest of the game (story, NPCs, saving, open world).

## Controls

| Action | Key |
|---|---|
| Move / look | WASD / mouse |
| Walk | Left Alt |
| Attack | Left click |
| Guard / deflect | Right click |
| Combat art | Left + right click |
| Step (hold: sprint) | Shift |
| Jump | Space |
| Lock on | Q |
| Prosthetic / switch | F / Z |
| Healing Gourd | E |
| Crouch | C |
| Sheathe | X |

`F1` debug menu (pick enemy, AI mode, cheats), `F3` frame rate, `R` reset,
`[` `]` time of day, `F5` reload `config.toml`, `Esc` pause.

## Setup

You need Windows, Sekiro from Steam (patch 1.06), Rust and Python 3
(Node.js optional, for the game's effect files).

1. Build the data from your game (takes a while):
   ```powershell
   powershell -ExecutionPolicy Bypass -File tools\extract.ps1
   ```
   Add `-Sekiro "D:\path\to\Sekiro"` if the game is not in the default Steam
   folder.
2. Run:
   ```bash
   cargo run --release
   ```

Tests: `cargo test --release`.

## Where things are

| Path | What |
|---|---|
| `src/player.rs` | Wolf |
| `src/enemy.rs`, `src/ai.rs` | Enemies and their AI scripts |
| `src/combat.rs` | Hits, posture, damage |
| `src/anim.rs`, `src/model.rs`, `src/cloth.rs` | Animation, models, cloth |
| `src/fxr.rs`, `src/sound.rs`, `src/hud.rs` | Effects, sound, HUD |
| `src/map.rs`, `src/camera.rs` | Maps, camera |
| `tools/sekiro-extract/` | Reads the game's files into `extracted/` |
| `docs/` | Notes on every mechanic and where it was found |

## License

Code under the [MIT License](LICENSE.md). It covers this code only, not the
game or anything generated from it. Please keep `extracted/` out of forks.
