// Converts Sekiro FXR effects to JSON with @cccode/fxr (public domain, github.com/EvenTorset/fxr).
//   node tools/fxr-dump/dump.mjs <ffxbnd.d dir> <out dir> <id> [<id> ...]
import { FXR, Game } from '@cccode/fxr'
import fs from 'node:fs'
import path from 'node:path'
const [dir, out, ...ids] = process.argv.slice(2)
fs.mkdirSync(out, { recursive: true })
for (const id of ids) {
  const file = path.join(dir, `f${String(id).padStart(9, '0')}.fxr`)
  if (!fs.existsSync(file)) { console.log(`${id}: missing`); continue }
  const fxr = await FXR.read(file, Game.Sekiro)
  fs.writeFileSync(path.join(out, `${id}.json`), JSON.stringify(fxr.toJSON(), null, 1))
  console.log(`${id}: ok`)
}
