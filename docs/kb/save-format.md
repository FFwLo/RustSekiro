# Sekiro's save file (S0000.sl2): first findings (2026-10-19)

Read-only: only ever a copy of the save, never the file in the Sekiro save folder.

- Container: BND4, 12 entries USER_DATA000..011, entry headers 0x20 bytes (flags 0x50, -1,
  size i64, data offset u32, name offset u32 (UTF-16), 8 zero bytes).
- Each entry: 16-byte MD5 of the rest, then the data **unencrypted** (checked: the MD5 matches
  on all 12; no AES as in DS3).
- USER_DATA000..009: the character slots (1 MiB each). USER_DATA010 (384 KiB): the profile
  (slot summaries, options). USER_DATA011: empty here.
- In a slot (offsets after the MD5, this save):
  - 0x34e00..: the weapon / protector instance table, 0x3c-byte records: handle (0x808000xx
    weapon, 0x908000xx protector), item id, ..., 1.
  - 0x8e700..: the inventory, 16-byte records: handle (0xb0000000 | goods id for goods),
    item id (0x40000000 | goods id; 0x10000000 protector; weapons plain), quantity, sort key.
    E.g. b0000bb8 / 40000bb8 / 1: the Healing Gourd (goods 3000); 70000 the Loaded Shuriken.
- Community pointers (to verify against the file and the exe, not taken as fact):
  alfizari/Sekiro-Save-Editor (Apache 2.0) lists HP 0x3446C, Attack Power 0x3449C, Sen 0x344D0,
  Skill Points 0x345B4, NG+ 0x33F34, inventory 0x8F70C (0x7000), storage 0x987A0 (0x9000).
  Its offsets likely count from the entry start (with the MD5): check before use.
- Open: event flags (the bit field at the slot's start is a candidate), position / map / last
  idol, the profile entry, the exe's save writer.
