import { useEffect, useState } from 'react'
import { BorderBeam } from 'border-beam'
import { Search, X } from 'lucide-react'
import type Database from '@tauri-apps/plugin-sql'
import { useReducedMotion } from '../../lib/useReducedMotion'

interface SearchHit { id: string; title: string; detail: string; category: string; destination: string }
interface NamedRow { id: string; title: string; detail: string }

export function DashboardSearch({ db, query, onQuery, onNavigate }: { db: Database; query: string; onQuery: (value: string) => void; onNavigate: (section: string) => void }) {
  const [focused, setFocused] = useState(false)
  const [hits, setHits] = useState<SearchHit[]>([])
  const [resultQuery, setResultQuery] = useState('')
  const [error, setError] = useState('')
  const reducedMotion = useReducedMotion()

  useEffect(() => {
    const term = query.trim()
    if (!term) return
    let cancelled = false
    const timer = window.setTimeout(() => {
      const pattern = `%${term}%`
      void Promise.all([
        db.select<NamedRow[]>("SELECT id,name AS title,('Project · '||status) AS detail FROM projects WHERE archived_at IS NULL AND (name LIKE ? OR description LIKE ?) ORDER BY updated_at DESC LIMIT 5", [pattern, pattern]).then(rows => rows.map(row => ({ ...row, category: 'Project', destination: 'Projects' }))),
        db.select<NamedRow[]>("SELECT t.id,t.title,('Task · '||t.status||' · '||COALESCE(p.name,'No project')) AS detail FROM tasks t LEFT JOIN projects p ON p.id=t.project_id WHERE t.archived_at IS NULL AND (t.title LIKE ? OR t.description LIKE ? OR t.blocker LIKE ?) ORDER BY t.updated_at DESC LIMIT 5", [pattern, pattern, pattern]).then(rows => rows.map(row => ({ ...row, category: 'Task', destination: 'Tasks' }))),
        db.select<NamedRow[]>("SELECT a.id,a.name AS title,('Account · '||b.name||' / '||bp.profile_name) AS detail FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id WHERE a.archived_at IS NULL AND (a.name LIKE ? OR COALESCE(a.identifier,'') LIKE ? OR a.notes LIKE ?) ORDER BY a.updated_at DESC LIMIT 5", [pattern, pattern, pattern]).then(rows => rows.map(row => ({ ...row, category: 'Account', destination: 'Accounts' }))),
        db.select<NamedRow[]>("SELECT id,title,('Reminder · '||status||' · '||due_at) AS detail FROM reminders WHERE title LIKE ? OR body LIKE ? ORDER BY due_at LIMIT 5", [pattern, pattern]).then(rows => rows.map(row => ({ ...row, category: 'Reminder', destination: 'Reminders' }))),
        db.select<NamedRow[]>("SELECT id,description AS title,('Activity · '||activity_type||' · '||occurred_at) AS detail FROM activity_logs WHERE description LIKE ? OR activity_type LIKE ? ORDER BY occurred_at DESC LIMIT 5", [pattern, pattern]).then(rows => rows.map(row => ({ ...row, category: 'Activity', destination: 'Activity' }))),
        db.select<NamedRow[]>("SELECT d.id,d.title,('Deliverable · '||COALESCE(p.name,'No project')) AS detail FROM deliverables d LEFT JOIN projects p ON p.id=d.project_id WHERE d.title LIKE ? OR d.notes LIKE ? ORDER BY d.completed_at DESC LIMIT 5", [pattern, pattern]).then(rows => rows.map(row => ({ ...row, category: 'Deliverable', destination: 'Output Vault' }))),
      ]).then(groups => { if (!cancelled) { setHits(groups.flat()); setResultQuery(term); setError('') } }).catch(value => { if (!cancelled) { setHits([]); setResultQuery(term); setError(String(value)) } })
    }, 140)
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [db, query])

  return <section className="dashboard-search" aria-label="Search workspace records">
    <div className="dashboard-search-heading"><span>WORKSPACE SEARCH</span><small>Projects · tasks · accounts · reminders · activity · deliverables</small></div>
    <BorderBeam size="line" colorVariant="colorful" strength={0.58} theme="dark" active={focused && !reducedMotion} className="dashboard-search-beam">
      <label className="dashboard-search-field"><Search size={19} aria-hidden="true"/><input value={query} onChange={event => onQuery(event.target.value)} onFocus={() => setFocused(true)} onBlur={() => setFocused(false)} placeholder="Search anything in LEVI…" aria-label="Search anything in LEVI"/><kbd>Ctrl K</kbd>{query && <button type="button" aria-label="Clear search" onMouseDown={event => event.preventDefault()} onClick={() => onQuery('')}><X size={16}/></button>}</label>
    </BorderBeam>
    {query.trim() && <div className="dashboard-search-results" role="region" aria-label="Search results" aria-live="polite">
      {resultQuery !== query.trim() ? <p>Searching saved records…</p> : error ? <p role="alert">Search is unavailable: {error}</p> : hits.length ? hits.map(hit => <button key={`${hit.category}:${hit.id}`} type="button" className="dashboard-search-hit" onClick={() => onNavigate(hit.destination)}><span>{hit.category}</span><strong>{hit.title}</strong><small>{hit.detail}</small></button>) : <p>No matching saved records.</p>}
    </div>}
  </section>
}
