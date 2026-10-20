# Credits

Sv1 recreates SEKIRO: SHADOWS DIE TWICE, which belongs to FromSoftware and Activision. This
project is not affiliated with or endorsed by them. Nothing from the game is in this
repository: every player builds the data from their own copy.

## Special thanks

**AKJama**, creator of [sekiro-rs](https://github.com/AKJama/sekiro-rs), another Sekiro
recreation in Rust. From their work:

- **Enemy AI:** the Lua 5.0 loader and interpreter (`src/ai/lua50.rs`) that runs the game's own
  compiled enemy battle scripts.
- **Player script:** the HKS bytecode VM (`sekiro-hks`, used as a library in tests) that runs the
  game's own compiled player script beside ours.
- **Combat rules:** hit stop, the Owl's deathblow gate, bullets fading past their range,
  deathblow positioning giving way at walls, enemy super armour, enemies' reactions to hits in
  the air, guarded bullets leaving their shooter alone, how prosthetic tool hits cause
  reactions, block versus deflect by repel values, the posture and HP a guard lets through,
  the posture ranges of a collapse, the counter-only mikiri hitbox and Attack Power growth.
- **Maps and lighting:** the light and fog colour rules, fog density, the camera's exposure
  metering and adaptation, the GI probe lighting, the bloom levels and the map material's
  batching, from values they traced in the game's exe and shaders. Our own sandbox and
  Construct are lit by their lighting library (light sets, time of day, fog, exposure, bloom,
  sun shadow), and its light units light our effects.
- **Sky and fog:** the sky domes (`src/sky.rs`, `src/sky.wgsl`) and the height fog
  (`src/height_fog.rs`, `src/height_fog.wgsl`) are adapted from their code.
- **World:** the story time of day, the boss fog walls, MSB region boxes and the arena start
  check.
- **Movement:** the grapple marker's grey disc for a point found but out of range.
- **Bosses:** the Headless Ape and its partner reading each other through the map's event
  target.

Where each one is used in the code is listed in [THIRD_PARTY.md](THIRD_PARTY.md).

## Code and data used

| Project | What Sv1 uses it for |
|---|---|
| [sekiro-rs](https://github.com/AKJama/sekiro-rs) (MIT / Apache-2.0) | Lua 5.0 interpreter (`src/ai/lua50.rs`), sky domes, height fog, ported rules (see THIRD_PARTY.md) |
| [@cccode/fxr](https://github.com/EvenTorset/fxr) (public domain) | Reads the game's effect files (FXR) for `tools/fxr-dump` |
| [DarkScript3](https://github.com/AinTunez/DarkScript3) | Sekiro's event-script instruction definitions (EMEVD argument types) |
| [Bevy](https://bevyengine.org) (MIT / Apache-2.0) | The game engine |
| The game's own FMOD runtime | Plays the game's sounds (loaded from your Sekiro install, not shipped) |

## References (read for file formats and names, no code copied)

- [SoulsFormatsNEXT](https://github.com/soulsmods/SoulsFormatsNEXT): FLVER, TAE, BND, TPF, MSB,
  param and other FromSoftware file formats.
- [Paramdex](https://github.com/soulsmods/Paramdex): param layouts and field names.
- [DSMapStudio](https://github.com/soulsmods/DSMapStudio) and
  [Smithbox](https://github.com/vawser/Smithbox): map and param editing references.
- [DSAnimStudio](https://github.com/Meowmaritus/DSAnimStudio): TAE event templates.
- [SekiroHKS](https://github.com/iitsigor/SekiroHKS) and
  [SDT-HKS](https://github.com/vawser/SDT-HKS): the character behaviour scripts' names.
- [ESDLang](https://github.com/thefifthmatt/ESDLang): state machine scripts.
- [SekiroTool](https://github.com/borgCode/SekiroTool): memory offsets, for the read-only
  checks against the running game.
- [gamedb](https://github.com/smileybaal/gamedb): indexing the exe decompile.
- The archive keys and file-name list in `tools/sekiro-extract/keys/` are the ones the
  community already ships with its unpacking tools (UXM).

Licences: this project's code is MIT ([LICENSE.md](LICENSE.md)); third-party licence texts
are in [THIRD_PARTY.md](THIRD_PARTY.md).
