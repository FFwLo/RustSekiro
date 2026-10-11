# Third-party code

## sekiro-rs

`src/ai/lua50.rs` (the Lua 5.0 loader and interpreter) is adapted from
[sekiro-rs](https://github.com/AKJama/sekiro-rs), `crates/sim/src/lua_ai/lua50.rs`,
which is dual-licensed MIT or Apache-2.0. It is used here under the MIT license:

```
MIT License

Copyright (c) 2026 AKJama

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Event instruction argument types

`tools/emedf_types.json` lists, per EMEVD instruction class and index, the argument types the
game's event scripts use (byte, short, int, float). It is a table of the binary format's facts,
generated from the Sekiro instruction definitions that ship with
[DarkScript3](https://github.com/AinTunez/DarkScript3) (`sekiro-common.emedf.json`, also at
https://soulsmods.github.io/emedf/sekiro-emedf.html); none of that file's names, descriptions
or text is included. `tools/boss_scripts.py` reads the definitions file itself when a
`reference/DarkScript3` clone is next to the repo.
