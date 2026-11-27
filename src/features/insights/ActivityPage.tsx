import { useCallback, useEffect, useMemo, useState } from 'react'
import type Database from '@tauri-apps/plugin-sql'
import { Search } from 'lucide-react'
import { errorMessage } from '../../lib/database'
import './insights.css'

interface Event { id: string; at: string; kind: string; text: string; account: string | null; project: string | null; task: string | null; origin: 'log' | 'task' }

const KIND_LABELS: Record<string, string> = { account_launch_requested: 'Account opened', account_archived: 'Account archived', levi_browser_open_requested: 'Opened in LEVI Browser', status: 'Task status', reminder: 'Reminder' }
const kindLabel = (kind: string) => KIND_LABELS[kind] ?? kind.replaceAll('_', ' ').replace(/^\w/, c => c.toUpperCase())
/** SQLite datetime('now') values are UTC without a zone marker. */
const toDate = (value: string) => new Date(`${value.replace(' ', 'T')}${/[zZ]|[+-]\d\d:?\d\d$/.test(value) ? '' : 'Z'}`)
const dayKey = (date: Date) => date.toLocaleDateString([], { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' })

/** Everything that happened in the workspace, newest first, from the activity log and task history. */
export function ActivityPage({ db }: { db: Database }) {
  const [events, setEvents] = useState<Event[] | null>(null)
  const [error, setError] = useState('')
  const [query, setQuery] = useState('')
  const [kind, setKind] = useState('all')

  const load = useCallback(async () => {
    const [logs, history] = await Promise.all([
      db.select<Event[]>(`SELECT l.id, l.occurred_at AS at, l.activity_type AS kind, l.description AS text, a.name AS account, p.name AS project, t.title AS task, 'log' AS origin
        FROM activity_logs l LEFT JOIN accounts a ON a.id = l.account_id LEFT JOIN projects p ON p.id = l.project_id LEFT JOIN tasks t ON t.id = l.task_id
        ORDER BY l.occurred_at DESC LIMIT 400`),
      db.select<Event[]>(`SELECT h.id, h.changed_at AS at, 'status' AS kind, 'Status ' || COALESCE(h.old_value, 'new') || ' → ' || COALESCE(h.new_value, '') AS text, NULL AS account, p.name AS project, t.title AS task, 'task' AS origin
        FROM task_history h JOIN tasks t ON t.id = h.task_id LEFT JOIN projects p ON p.id = t.project_id
        WHERE h.field = 'status' ORDER BY h.changed_at DESC LIMIT 400`),
    ])
    setEvents([...logs, ...history].sort((a, b) => toDate(b.at).getTime() - toDate(a.at).getTime()).slice(0, 500))
  }, [db])
  // oxlint-disable-next-line react/set-state-in-effect -- loading saved history when the page opens
  useEffect(() => { void load().catch(e => setError(errorMessage(e))) }, [load])

  const kinds = useMemo(() => [...new Set((events ?? []).map(event => event.kind))].sort(), [events])
  const shown = useMemo(() => (events ?? []).filter(event => (kind === 'all' || event.kind === kind) && `${event.text} ${event.account ?? ''} ${event.project ?? ''} ${event.task ?? ''}`.toLowerCase().includes(query.toLowerCase())), [events, kind, query])
  const days = useMemo(() => {
    const groups = new Map<string, Event[]>()
    for (const event of shown) { const key = dayKey(toDate(event.at)); groups.set(key, [...(groups.get(key) ?? []), event]) }
    return [...groups]
  }, [shown])

  return <div className="content insights-page">
    <div className="feature-header"><div><div className="eyebrow">INSIGHT / ACTIVITY</div><h1>Activity</h1><p>What happened in your workspace, newest first. Everything here was recorded as it happened.</p></div></div>
    {error && <div className="inline-error" role="alert">{error}</div>}
    <div className="task-toolbar"><label className="searchbox"><Search size={15}/><input value={query} onChange={event => setQuery(event.target.value)} placeholder="Search activity"/></label>
      <select value={kind} onChange={event => setKind(event.target.value)} aria-label="Kind of activity"><option value="all">Everything</option>{kinds.map(item => <option key={item} value={item}>{kindLabel(item)}</option>)}</select></div>
    {!events ? <div className="insight-skeletons"><div className="skeleton"/><div className="skeleton"/><div className="skeleton"/></div>
      : !days.length ? <div className="list-empty"><div><strong>{events.length ? 'Nothing matches' : 'No activity yet'}</strong><span>{events.length ? 'Try another search or kind.' : 'Opening accounts, changing tasks and reminders show up here.'}</span></div></div>
      : days.map(([day, items]) => <section className="activity-day" key={day}>
        <h2>{day}</h2>
        <ol className="activity-list">{items.map(event => <li key={`${event.origin}-${event.id}`}>
          <time>{toDate(event.at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time>
          <span className={`activity-kind kind-${event.origin}`}>{kindLabel(event.kind)}</span>
          <span className="activity-text">{event.task ? <strong>{event.task}</strong> : null}{event.task ? ' · ' : ''}{event.text}</span>
          <span className="activity-tags">{[event.project, event.account].filter(Boolean).join(' · ')}</span>
        </li>)}</ol>
      </section>)}
  </div>
}
