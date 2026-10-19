import fs from 'node:fs'
import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig, type Plugin } from 'vite'

// The atlas research data in docs/kb (repo root = three levels up).
const kb = path.resolve(import.meta.dirname, '../../docs/kb')

// Serves the research data: /data/atlas.json (the merged atlas once written, else the
// research parts joined) and /data/progress.json (the progress log's lines).
function atlasData(): Plugin {
  const json = (res: import('node:http').ServerResponse, body: unknown) => {
    res.setHeader('Content-Type', 'application/json')
    res.setHeader('Cache-Control', 'no-store')
    res.end(JSON.stringify(body))
  }
  const read = (file: string) => {
    try {
      return JSON.parse(fs.readFileSync(file, 'utf8'))
    } catch {
      return null
    }
  }
  return {
    name: 'atlas-data',
    configureServer(server) {
      server.middlewares.use('/data/atlas.json', (_req, res) => {
        const merged = read(path.join(kb, 'mechanics-atlas.json'))
        if (Array.isArray(merged)) return json(res, merged)
        const dir = path.join(kb, 'atlas', 'parts')
        const files = fs.existsSync(dir) ? fs.readdirSync(dir).filter((f) => f.endsWith('.json')) : []
        json(res, files.flatMap((f) => read(path.join(dir, f)) ?? []))
      })
      server.middlewares.use('/data/progress.json', (_req, res) => {
        let text = ''
        try {
          text = fs.readFileSync(path.join(kb, 'atlas', 'progress.jsonl'), 'utf8')
        } catch {
          /* no log yet */
        }
        const lines = text.split('\n').flatMap((l) => {
          try {
            return l.trim() ? [JSON.parse(l)] : []
          } catch {
            return []
          }
        })
        json(res, lines)
      })
    },
  }
}

export default defineConfig({
  plugins: [react(), tailwindcss(), atlasData()],
  resolve: { alias: { '@': path.resolve(import.meta.dirname, './src') } },
  server: { port: 5173, strictPort: true },
})
