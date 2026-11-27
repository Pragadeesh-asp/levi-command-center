import { useCallback, useEffect, useRef, useState } from 'react'
import type Database from '@tauri-apps/plugin-sql'
import { Bell } from 'lucide-react'

interface Item { key: string; kind: 'reminder' | 'overdue' | 'handoff' | 'limit'; title: string; detail: string; section: string }

const KIND_LABEL: Record<Item['kind'], string> = { reminder: 'Reminder due', overdue: 'Overdue task', handoff: 'Handoff waiting', limit: 'Limit reset passed' }

/**
 * The bell: what needs you right now, from saved records. Checked every minute.
 * Opening an item goes to the section where it can be dealt with.
 */
export function NotificationBell({ db, onNavigate }: { db: Database | null; onNavigate: (section: string) => void }) {
  const [items, setItems] = useState<Item[]>([])
  const [open, setOpen] = useState(false)
  const root = useRef<HTMLDivElement>(null)

  const load = useCallback(async () => {
    if (!db) return
    const [reminders, overdue, handoffs, limits] = await Promise.all([
      db.select<{ id: string; title: string; due_at: string }[]>("SELECT id, title, due_at FROM reminders WHERE status IN ('pending','snoozed') AND ((status='pending' AND due_at <= datetime('now')) OR (status='snoozed' AND snoozed_until <= datetime('now'))) ORDER BY due_at DESC LIMIT 20"),
      db.select<{ id: string; title: string; deadline: string }[]>("SELECT id, title, deadline FROM tasks WHERE archived_at IS NULL AND status != 'completed' AND deadline IS NOT NULL AND deadline < datetime('now') ORDER BY deadline LIMIT 20"),
      db.select<{ id: string; title: string; status: string }[]>("SELECT h.id, t.title, h.status FROM task_handoffs h JOIN tasks t ON t.id = h.task_id WHERE h.status IN ('prepared','revision_required','submitted') ORDER BY h.created_at DESC LIMIT 20"),
      db.select<{ id: string; name: string }[]>("SELECT e.id, a.name FROM usage_limit_events e JOIN accounts a ON a.id = e.account_id WHERE e.status = 'waiting' AND e.reset_at IS NOT NULL AND datetime(e.reset_at) <= datetime('now', 'localtime')"),
    ])
    setItems([
      ...reminders.map(r => ({ key: `r${r.id}`, kind: 'reminder' as const, title: r.title, detail: `Due ${r.due_at}`, section: 'Reminders' })),
      ...overdue.map(t => ({ key: `t${t.id}`, kind: 'overdue' as const, title: t.title, detail: `Deadline ${t.deadline}`, section: 'Tasks' })),
      ...handoffs.map(h => ({ key: `h${h.id}`, kind: 'handoff' as const, title: h.title, detail: h.status.replaceAll('_', ' '), section: 'Handoff Center' })),
      ...limits.map(l => ({ key: `l${l.id}`, kind: 'limit' as const, title: l.name, detail: 'Confirm the resume in Checkpoints', section: 'Checkpoint Center' })),
    ])
  }, [db])

  useEffect(() => {
    // oxlint-disable-next-line react/set-state-in-effect -- polling saved records for the bell
    void load().catch(() => undefined)
    const timer = window.setInterval(() => void load().catch(() => undefined), 60000)
    return () => window.clearInterval(timer)
  }, [load])

  useEffect(() => {
    if (!open) return
    const onDown = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false) }
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') { event.stopImmediatePropagation(); setOpen(false) } }
    window.addEventListener('pointerdown', onDown)
    window.addEventListener('keydown', onKey, true)
    return () => { window.removeEventListener('pointerdown', onDown); window.removeEventListener('keydown', onKey, true) }
  }, [open])

  return <div className="levi-bell" ref={root}>
    <button type="button" className="icon-button levi-bell-button" aria-label={items.length ? `${items.length} things need you` : 'Nothing needs you'} aria-expanded={open} onClick={() => { setOpen(value => !value); void load().catch(() => undefined) }}>
      <Bell size={17}/>{items.length > 0 && <span className="levi-bell-count">{items.length > 9 ? '9+' : items.length}</span>}
    </button>
    {open && <div className="levi-bell-panel" role="dialog" aria-label="Notifications">
      <header>{items.length ? `${items.length} need${items.length === 1 ? 's' : ''} you` : 'All clear'}</header>
      {items.length === 0 ? <p className="levi-bell-empty">No due reminders, overdue tasks, waiting handoffs or passed limit resets.</p>
        : <ul>{items.map(item => <li key={item.key}><button type="button" onClick={() => { setOpen(false); onNavigate(item.section) }}>
          <span className={`levi-bell-kind kind-${item.kind}`}>{KIND_LABEL[item.kind]}</span><strong>{item.title}</strong><small>{item.detail}</small>
        </button></li>)}</ul>}
    </div>}
  </div>
}
