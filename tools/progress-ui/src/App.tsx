import { useEffect, useMemo, useState } from "react"
import { FunnelChart } from "@/components/charts/funnel-chart"
import { BarChart } from "@/components/charts/bar-chart"
import { Bar } from "@/components/charts/bar"
import { BarXAxis } from "@/components/charts/bar-x-axis"
import { AreaChart, Area } from "@/components/charts/area-chart"
import { XAxis } from "@/components/charts/x-axis"
import { Grid } from "@/components/charts/grid"
import { ChartTooltip } from "@/components/charts/tooltip"

type Has = { has?: string; where?: string }
type Rec = {
  id: string
  category: string
  title: string
  summary?: string
  values?: string
  how?: string
  sources?: { kind: string; ref: string }[]
  confidence?: string
  ours?: Has
  his?: Has
  needs?: { id: string; why?: string }[]
  check?: string
  note?: string
  verdict?: string
  polished?: boolean
  published?: boolean
}
type Log = { t: string; step: string; agent?: string; category?: string; status?: string; added?: number; [k: string]: unknown }

const CATEGORIES = [
  "movement", "combat", "armor", "deathblows", "arts", "items", "status", "ai", "stealth", "bosses",
  "camera", "map", "visuals", "sound", "progression", "interface", "npcs", "cutscenes", "dialogue", "frontend", "flow",
]
const CONF = [
  { key: "data", color: "#4f8a6b" },
  { key: "exe", color: "#3f7fa6" },
  { key: "recorded", color: "#8a6bb5" },
  { key: "community", color: "#c49a3a" },
  { key: "open-question", color: "#c0573f" },
]
const has = (h?: Has) => h?.has === "yes" || h?.has === "partial"

async function getJson<T>(url: string, fallback: T): Promise<T> {
  try {
    const r = await fetch(url, { cache: "no-store" })
    return r.ok ? ((await r.json()) as T) : fallback
  } catch {
    return fallback
  }
}

function useData() {
  const [records, setRecords] = useState<Rec[]>([])
  const [log, setLog] = useState<Log[]>([])
  const [at, setAt] = useState<Date | null>(null)
  useEffect(() => {
    let live = true
    const load = async () => {
      const [a, p] = await Promise.all([getJson<Rec[]>("/data/atlas.json", []), getJson<Log[]>("/data/progress.json", [])])
      if (!live) return
      setRecords(Array.isArray(a) ? a : [])
      setLog(Array.isArray(p) ? p : [])
      setAt(new Date())
    }
    load()
    const id = setInterval(load, 10_000)
    return () => {
      live = false
      clearInterval(id)
    }
  }, [])
  return { records, log, at }
}

function useDarkClass() {
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)")
    const set = () => document.documentElement.classList.toggle("dark", mq.matches)
    set()
    mq.addEventListener("change", set)
    return () => mq.removeEventListener("change", set)
  }, [])
}

/** Dependency view: which needs resolve, which records are blocked in each build, what to build first. */
function useDeps(records: Rec[]) {
  return useMemo(() => {
    const byId = new Map(records.map((r) => [r.id, r]))
    const neededBy = new Map<string, string[]>()
    const dangling: { from: string; id: string }[] = []
    for (const r of records)
      for (const n of r.needs ?? []) {
        if (!byId.has(n.id)) dangling.push({ from: r.id, id: n.id })
        neededBy.set(n.id, [...(neededBy.get(n.id) ?? []), r.id])
      }
    const blocked = (side: "ours" | "his") =>
      records
        .map((r) => ({ r, missing: (r.needs ?? []).filter((n) => !has(byId.get(n.id)?.[side])).map((n) => n.id) }))
        .filter((x) => x.missing.length > 0)
    const buildFirst = records
      .filter((r) => !has(r.ours))
      .map((r) => ({ r, count: neededBy.get(r.id)?.length ?? 0 }))
      .filter((x) => x.count > 0)
      .sort((a, b) => b.count - a.count)
    return { byId, neededBy, dangling, blockedOurs: blocked("ours"), blockedHis: blocked("his"), buildFirst }
  }, [records])
}

function Panel({ title, sub, children, className = "" }: { title: string; sub?: string; children: React.ReactNode; className?: string }) {
  return (
    <section className={`rounded-xl border border-border bg-card p-4 min-w-0 ${className}`}>
      <h2 className="text-sm font-semibold tracking-wide uppercase text-muted-foreground">{title}</h2>
      {sub && <p className="text-xs text-muted-foreground mt-0.5">{sub}</p>}
      <div className="mt-3">{children}</div>
    </section>
  )
}

function Empty({ text }: { text: string }) {
  return <p className="text-sm text-muted-foreground py-8 text-center">{text}</p>
}

function Legend({ items }: { items: { key: string; color: string }[] }) {
  return (
    <div className="flex flex-wrap gap-3 text-xs text-muted-foreground mt-2">
      {items.map((i) => (
        <span key={i.key} className="inline-flex items-center gap-1.5">
          <span className="size-2.5 rounded-sm" style={{ background: i.color }} />
          {i.key}
        </span>
      ))}
    </div>
  )
}

function Overview({ records, log }: { records: Rec[]; log: Log[] }) {
  const deps = useDeps(records)
  const funnel = [
    { label: "Found", value: records.length },
    { label: "Verified", value: records.filter((r) => r.verdict === "confirmed" || r.verdict === "corrected").length },
    { label: "Polished", value: records.filter((r) => r.polished).length },
    { label: "Published", value: records.filter((r) => r.published).length },
  ]
  const cats = CATEGORIES.filter((c) => records.some((r) => r.category === c))
  const byCat = cats.map((c) => {
    const rs = records.filter((r) => r.category === c)
    const row: Record<string, unknown> = { category: c }
    for (const k of CONF) row[k.key] = rs.filter((r) => (r.confidence ?? "open-question") === k.key).length
    return row
  })
  const coverage = cats.map((c) => {
    const rs = records.filter((r) => r.category === c)
    return {
      category: c,
      ours: rs.filter((r) => has(r.ours)).length,
      his: rs.filter((r) => has(r.his)).length,
      neither: rs.filter((r) => !has(r.ours) && !has(r.his)).length,
    }
  })
  let total = 0
  const overTime = log
    .filter((l) => typeof l.added === "number")
    .sort((a, b) => a.t.localeCompare(b.t))
    .map((l) => ({ date: new Date(l.t), records: (total += l.added as number) }))
  const done = new Set(log.filter((l) => typeof l.added === "number").map((l) => `${l.agent}/${l.category}`))
  const running = log.filter((l) => l.status === "started" && !done.has(`${l.agent}/${l.category}`))

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Panel title="Pipeline" sub="found → verified → polished → published">
        {records.length ? (
          <FunnelChart data={funnel} color="#3f7fa6" showPercentage showValues className="h-56" />
        ) : (
          <Empty text="No records yet. The research agents are still writing." />
        )}
      </Panel>
      <Panel title="Activity" sub={running.length ? `Running: ${running.map((l) => `${l.agent} (${l.category})`).join(", ")}` : "No agent running"}>
        <ol className="space-y-1.5 text-sm max-h-56 overflow-y-auto tabular-nums">
          {[...log].reverse().slice(0, 30).map((l, i) => (
            <li key={i} className="flex gap-3">
              <span className="text-muted-foreground shrink-0">{l.t.slice(11, 16)}</span>
              <span className="min-w-0">
                {l.step} {l.agent && <b>{l.agent}</b>} {l.category}{" "}
                {l.status ?? (typeof l.added === "number" ? `+${l.added} records` : "")}
              </span>
            </li>
          ))}
        </ol>
      </Panel>
      <Panel title="Confidence by category" className="lg:col-span-2">
        {byCat.length ? (
          <>
            <BarChart data={byCat} xDataKey="category" stacked aspectRatio="3 / 1">
              <Grid horizontal />
              {CONF.map((k) => (
                <Bar key={k.key} dataKey={k.key} fill={k.color} lineCap="butt" />
              ))}
              <BarXAxis showAllLabels />
              <ChartTooltip />
            </BarChart>
            <Legend items={CONF} />
          </>
        ) : (
          <Empty text="Waiting for the first category file." />
        )}
      </Panel>
      <Panel title="Coverage" sub="how many mechanics each build has (yes or partial)">
        {coverage.length ? (
          <>
            <BarChart data={coverage} xDataKey="category" aspectRatio="2 / 1">
              <Grid horizontal />
              <Bar dataKey="ours" fill="#3f7fa6" />
              <Bar dataKey="his" fill="#c49a3a" />
              <Bar dataKey="neither" fill="#c0573f" />
              <BarXAxis showAllLabels />
              <ChartTooltip />
            </BarChart>
            <Legend items={[{ key: "RustSekiro", color: "#3f7fa6" }, { key: "sekiro-rs", color: "#c49a3a" }, { key: "neither", color: "#c0573f" }]} />
          </>
        ) : (
          <Empty text="No coverage tags yet." />
        )}
      </Panel>
      <Panel title="Records over time">
        {overTime.length >= 2 ? (
          <AreaChart data={overTime} aspectRatio="2 / 1">
            <Grid horizontal />
            <Area dataKey="records" fill="#3f7fa6" fillOpacity={0.35} />
            <XAxis />
            <ChartTooltip />
          </AreaChart>
        ) : (
          <Empty text="Needs two finished categories to draw a line." />
        )}
      </Panel>
      <Panel title="Dependencies" sub="a mechanic is blocked when something it needs is missing" className="lg:col-span-2">
        <div className="grid gap-4 md:grid-cols-3 text-sm">
          <DepList title={`Blocked in RustSekiro (${deps.blockedOurs.length})`} rows={deps.blockedOurs.map((x) => ({ id: x.r.id, text: `needs ${x.missing.join(", ")}` }))} />
          <DepList title={`Build first (${deps.buildFirst.length})`} rows={deps.buildFirst.map((x) => ({ id: x.r.id, text: `missing, needed by ${x.count}` }))} />
          <DepList title={`Broken links (${deps.dangling.length})`} rows={deps.dangling.map((d) => ({ id: d.from, text: `points at unknown ${d.id}` }))} />
        </div>
      </Panel>
    </div>
  )
}

function DepList({ title, rows }: { title: string; rows: { id: string; text: string }[] }) {
  return (
    <div className="min-w-0">
      <h3 className="font-medium mb-1.5">{title}</h3>
      {rows.length ? (
        <ul className="space-y-1 max-h-64 overflow-y-auto">
          {rows.slice(0, 80).map((r, i) => (
            <li key={i} className="min-w-0 break-words">
              <code className="text-xs">{r.id}</code> <span className="text-muted-foreground">{r.text}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-muted-foreground">None.</p>
      )}
    </div>
  )
}

const badge = (c?: string) => CONF.find((k) => k.key === c)?.color ?? "#888"
const needsRecording = (r: Rec) => r.confidence === "open-question" && /record|clip/i.test(`${r.note} ${r.check}`)

function Browser({ records }: { records: Rec[] }) {
  const deps = useDeps(records)
  const [q, setQ] = useState("")
  const [cat, setCat] = useState("")
  const [only, setOnly] = useState("")
  const shown = records.filter((r) => {
    if (cat && r.category !== cat) return false
    if (only === "missing-ours" && has(r.ours)) return false
    if (only === "missing-both" && (has(r.ours) || has(r.his))) return false
    if (only === "recording" && !needsRecording(r)) return false
    if (only && CONF.some((k) => k.key === only) && r.confidence !== only) return false
    const s = q.trim().toLowerCase()
    return !s || `${r.id} ${r.title} ${r.summary} ${r.values} ${r.how}`.toLowerCase().includes(s)
  })
  const field = "rounded-md border border-border bg-background px-2 py-1.5 text-sm"
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2">
        <input className={`${field} flex-1 min-w-48`} placeholder="Search (hyper armor, deflect, fall…)" value={q} onChange={(e) => setQ(e.target.value)} />
        <select className={field} value={cat} onChange={(e) => setCat(e.target.value)}>
          <option value="">All categories</option>
          {CATEGORIES.map((c) => (
            <option key={c}>{c}</option>
          ))}
        </select>
        <select className={field} value={only} onChange={(e) => setOnly(e.target.value)}>
          <option value="">Everything</option>
          <option value="missing-ours">Missing in RustSekiro</option>
          <option value="missing-both">Missing in both</option>
          <option value="recording">Needs a recording</option>
          {CONF.map((k) => (
            <option key={k.key} value={k.key}>
              Confidence: {k.key}
            </option>
          ))}
        </select>
      </div>
      <p className="text-xs text-muted-foreground tabular-nums">{shown.length} of {records.length} records</p>
      <ul className="space-y-2">
        {shown.slice(0, 300).map((r) => (
          <li key={r.id} className="rounded-lg border border-border p-3 min-w-0">
            <div className="flex flex-wrap items-baseline gap-2">
              <span className="font-medium">{r.title}</span>
              <code className="text-xs text-muted-foreground">{r.id}</code>
              <span className="text-[11px] px-1.5 rounded text-white" style={{ background: badge(r.confidence) }}>{r.confidence}</span>
              {r.verdict && <span className="text-[11px] px-1.5 rounded border border-border">{r.verdict}</span>}
              <span className="text-xs text-muted-foreground ml-auto">
                ours {r.ours?.has ?? "?"} · his {r.his?.has ?? "?"}
              </span>
            </div>
            {r.summary && <p className="text-sm mt-1">{r.summary}</p>}
            {r.values && <p className="text-xs mt-1 break-words"><b>Values:</b> {r.values}</p>}
            {r.how && <p className="text-xs break-words"><b>How:</b> {r.how}</p>}
            {!!r.needs?.length && <p className="text-xs break-words"><b>Needs:</b> {r.needs.map((n) => n.id).join(", ")}</p>}
            {!!deps.neededBy.get(r.id)?.length && <p className="text-xs break-words"><b>Needed by:</b> {deps.neededBy.get(r.id)!.join(", ")}</p>}
            {r.check && <p className="text-xs break-words"><b>Check:</b> {r.check}</p>}
            {r.note && <p className="text-xs text-muted-foreground break-words">{r.note}</p>}
            {!!r.sources?.length && (
              <p className="text-xs text-muted-foreground break-all">
                {r.sources.map((s, i) => (
                  <span key={i}>
                    {i > 0 && " · "}
                    {s.kind}: {s.ref.startsWith("http") ? <a className="underline" href={s.ref} target="_blank">{s.ref}</a> : s.ref}
                  </span>
                ))}
              </p>
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}

export default function App() {
  useDarkClass()
  const { records, log, at } = useData()
  const [tab, setTab] = useState<"overview" | "atlas">("overview")
  const tabBtn = (t: typeof tab, label: string) => (
    <button
      onClick={() => setTab(t)}
      className={`px-3 py-1.5 rounded-md text-sm ${tab === t ? "bg-foreground text-background" : "text-muted-foreground hover:text-foreground"}`}
    >
      {label}
    </button>
  )
  return (
    <div className="min-h-screen bg-background text-foreground">
      <div className="mx-auto max-w-6xl px-4 py-6 space-y-4">
        <header className="flex flex-wrap items-end gap-3">
          <div>
            <h1 className="text-xl font-semibold">Sekiro Mechanics Atlas: progress</h1>
            <p className="text-xs text-muted-foreground tabular-nums">
              {records.length} records · {log.length} log lines · refreshed {at ? at.toLocaleTimeString() : "…"} (every 10 s)
            </p>
          </div>
          <nav className="ml-auto flex gap-1">
            {tabBtn("overview", "Overview")}
            {tabBtn("atlas", "Atlas")}
          </nav>
        </header>
        {tab === "overview" ? <Overview records={records} log={log} /> : <Browser records={records} />}
      </div>
    </div>
  )
}
