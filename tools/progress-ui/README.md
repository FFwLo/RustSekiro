# progress-ui

Local dashboard for the Sekiro Mechanics Atlas research, built with Bklit charts (shadcn registry `@bklit`).

It reads `docs/kb/mechanics-atlas.json` (or, before the merge, `docs/kb/atlas/parts/*.json`) and
`docs/kb/atlas/progress.jsonl`, and re-reads them every 10 s.

```bash
npm install
npm run dev
```

Then open http://localhost:5173.

`src/components/charts/bar-chart.tsx` carries one local fix: vertical stacked bars scale to the stack total.
