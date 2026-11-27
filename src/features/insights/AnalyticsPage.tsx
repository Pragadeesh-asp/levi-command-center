import { useCallback, useEffect, useState } from 'react'
import type Database from '@tauri-apps/plugin-sql'
import { errorMessage } from '../../lib/database'
import './insights.css'

interface Bar { label: string; value: number; note?: string }
interface Data { weekly: Bar[]; byStatus: Bar[]; projects: { name: string; done: number; total: number }[]; limits: Bar[]; tokens: Bar[]; totals: { completed30: number; open: number; limitHits30: number; tokens14: number } }

/** Monday of the week containing `date`, as YYYY-MM-DD in local time. */
function weekStart(date: Date) {
  const d = new Date(date.getFullYear(), date.getMonth(), date.getDate())
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7))
  return d
}
const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
const short = (d: Date) => d.toLocaleDateString([], { day: 'numeric', month: 'short' })

/** Bars drawn to one scale, each with its exact value on hover and for screen readers. */
function BarChart({ bars, unit, highlightLast = false }: { bars: Bar[]; unit: string; highlightLast?: boolean }) {
  const max = Math.max(1, ...bars.map(bar => bar.value))
  return <div className="bar-chart" role="list">
    {bars.map((bar, index) => <div className="bar-col" role="listitem" key={bar.label} title={`${bar.label}: ${bar.value} ${unit}${bar.note ? ` · ${bar.note}` : ''}`}>
      <span className="bar-value">{bar.value || ''}</span>
      <span className={`bar ${highlightLast && index === bars.length - 1 ? 'is-current' : ''}`} style={{ height: `${(bar.value / max) * 100}%` }}/>
      <span className="bar-label">{bar.label}</span>
      <span className="sr-only">{bar.label}: {bar.value} {unit}</span>
    </div>)}
  </div>
}

/** Completion trends and usage from the local database. Nothing here is estimated. */
export function AnalyticsPage({ db }: { db: Database }) {
  const [data, setData] = useState<Data | null>(null)
  const [error, setError] = useState('')

  const load = useCallback(async () => {
    const now = new Date()
    const first = weekStart(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 7 * 7))
    const [completed, statuses, projects, limits, tokens] = await Promise.all([
      db.select<{ day: string }[]>("SELECT date(completed_at, 'localtime') AS day FROM tasks WHERE completed_at IS NOT NULL AND archived_at IS NULL AND date(completed_at, 'localtime') >= ?", [iso(first)]),
      db.select<{ status: string; n: number }[]>("SELECT status, COUNT(*) AS n FROM tasks WHERE archived_at IS NULL AND status != 'completed' GROUP BY status"),
      db.select<{ name: string; done: number; total: number }[]>("SELECT p.name, SUM(CASE WHEN t.status = 'completed' THEN 1 ELSE 0 END) AS done, COUNT(t.id) AS total FROM projects p JOIN tasks t ON t.project_id = p.id AND t.archived_at IS NULL WHERE p.archived_at IS NULL GROUP BY p.id ORDER BY total DESC LIMIT 8"),
      db.select<{ name: string; n: number }[]>("SELECT a.name, COUNT(e.id) AS n FROM accounts a LEFT JOIN usage_limit_events e ON e.account_id = a.id AND e.detected_at >= datetime('now', '-30 days') WHERE a.archived_at IS NULL GROUP BY a.id ORDER BY a.name"),
      db.select<{ day: string; n: number }[]>("SELECT date(created_at, 'localtime') AS day, COALESCE(SUM(total_tokens), 0) AS n FROM ai_requests WHERE created_at >= datetime('now', '-14 days') GROUP BY day"),
    ])
    const weekly: Bar[] = []
    for (let i = 0; i < 8; i++) {
      const start = new Date(first); start.setDate(first.getDate() + i * 7)
      const end = new Date(start); end.setDate(start.getDate() + 6)
      const value = completed.filter(row => row.day >= iso(start) && row.day <= iso(end)).length
      weekly.push({ label: short(start), value, note: `week of ${iso(start)}` })
    }
    const order = ['backlog', 'ready', 'in_progress', 'waiting', 'review', 'blocked']
    const byStatus = order.map(status => ({ label: status.replaceAll('_', ' '), value: statuses.find(row => row.status === status)?.n ?? 0 }))
    const tokenBars: Bar[] = []
    for (let i = 13; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i)
      tokenBars.push({ label: short(d), value: tokens.find(row => row.day === iso(d))?.n ?? 0 })
    }
    const thirtyAgo = iso(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 30))
    setData({
      weekly, byStatus, projects,
      limits: limits.map(row => ({ label: row.name, value: row.n })),
      tokens: tokenBars,
      totals: { completed30: completed.filter(row => row.day >= thirtyAgo).length, open: byStatus.reduce((n, bar) => n + bar.value, 0), limitHits30: limits.reduce((n, row) => n + row.n, 0), tokens14: tokenBars.reduce((n, bar) => n + bar.value, 0) },
    })
  }, [db])
  // oxlint-disable-next-line react/set-state-in-effect -- loading saved records when the page opens
  useEffect(() => { void load().catch(e => setError(errorMessage(e))) }, [load])

  return <div className="content insights-page">
    <div className="feature-header"><div><div className="eyebrow">INSIGHT / ANALYTICS</div><h1>Analytics</h1><p>Trends from your saved tasks, projects, accounts and JARVIS requests.</p></div></div>
    {error && <div className="inline-error" role="alert">{error}</div>}
    {!data ? <div className="insight-skeletons"><div className="skeleton tall"/><div className="skeleton tall"/></div> : <>
      <section className="insight-totals">
        <div><span>Finished in 30 days</span><strong>{data.totals.completed30}</strong></div>
        <div><span>Open tasks</span><strong>{data.totals.open}</strong></div>
        <div><span>Limit hits in 30 days</span><strong>{data.totals.limitHits30}</strong></div>
        <div><span>JARVIS tokens, 14 days</span><strong>{data.totals.tokens14.toLocaleString()}</strong></div>
      </section>
      <div className="insight-grid">
        <section className="panel insight-panel"><h2>Tasks finished per week</h2><p>Last 8 weeks; this week is highlighted.</p><BarChart bars={data.weekly} unit="tasks" highlightLast/></section>
        <section className="panel insight-panel"><h2>Open work by status</h2><p>Every task not yet completed.</p><BarChart bars={data.byStatus} unit="tasks"/></section>
        <section className="panel insight-panel"><h2>Projects</h2><p>Completed out of all tasks in each project.</p>
          {data.projects.length ? <ul className="progress-list">{data.projects.map(project => <li key={project.name}><span>{project.name}</span><span className="progress-track"><span style={{ width: `${project.total ? (project.done / project.total) * 100 : 0}%` }}/></span><span className="progress-count">{project.done}/{project.total}</span></li>)}</ul> : <p className="insight-empty">No project has tasks yet.</p>}
        </section>
        <section className="panel insight-panel"><h2>Usage-limit hits</h2><p>Per Claude account, last 30 days.</p>{data.limits.length ? <BarChart bars={data.limits} unit="hits"/> : <p className="insight-empty">No accounts yet.</p>}</section>
        <section className="panel insight-panel wide"><h2>JARVIS tokens per day</h2><p>As reported by the provider, last 14 days.</p><BarChart bars={data.tokens} unit="tokens" highlightLast/></section>
      </div>
    </>}
  </div>
}
