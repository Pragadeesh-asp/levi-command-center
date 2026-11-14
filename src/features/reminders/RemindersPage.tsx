import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification'
import type Database from '@tauri-apps/plugin-sql'
import { Bell, Check, Clock3, Plus, RefreshCw, X } from 'lucide-react'
import { errorMessage, newId } from '../../lib/database'
import '../workforce/workforce.css'
import './reminders.css'

interface Reminder { id: string; title: string; body: string; kind: string; due_at: string; status: string; snoozed_until: string | null; delivered_at: string | null; delivery_attempts: number; last_delivery_error: string | null; account_name: string | null; project_name: string | null; task_title: string | null }
interface LinkOption { id: string; name: string }
interface Form { title: string; body: string; kind: string; due_at: string; account_id: string; project_id: string; task_id: string }
const empty: Form = { title: '', body: '', kind: 'one_time', due_at: '', account_id: '', project_id: '', task_id: '' }

export function RemindersPage({ db }: { db: Database }) {
  const [rows, setRows] = useState<Reminder[]>([])
  const [projects, setProjects] = useState<LinkOption[]>([])
  const [accounts, setAccounts] = useState<LinkOption[]>([])
  const [tasks, setTasks] = useState<LinkOption[]>([])
  const [form, setForm] = useState<Form>(empty)
  const [creating, setCreating] = useState(false)
  const [enabled, setEnabled] = useState(false)
  const [permission, setPermission] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)

  const reload = useCallback(async () => {
    const [reminders, projectRows, accountRows, taskRows, setting, permitted] = await Promise.all([
      db.select<Reminder[]>(`SELECT r.id,r.title,r.body,r.kind,r.due_at,r.status,r.snoozed_until,r.delivered_at,r.delivery_attempts,r.last_delivery_error,a.name AS account_name,p.name AS project_name,t.title AS task_title FROM reminders r LEFT JOIN accounts a ON a.id=r.account_id LEFT JOIN projects p ON p.id=r.project_id LEFT JOIN tasks t ON t.id=r.task_id ORDER BY CASE r.status WHEN 'pending' THEN 0 WHEN 'snoozed' THEN 1 ELSE 2 END,COALESCE(r.snoozed_until,r.due_at),r.created_at DESC`),
      db.select<LinkOption[]>('SELECT id,name FROM projects WHERE archived_at IS NULL AND status="active" ORDER BY name COLLATE NOCASE'),
      db.select<LinkOption[]>('SELECT id,name FROM accounts WHERE archived_at IS NULL ORDER BY name COLLATE NOCASE'),
      db.select<LinkOption[]>('SELECT id,title AS name FROM tasks WHERE archived_at IS NULL ORDER BY title COLLATE NOCASE'),
      invoke<boolean>('get_reminder_notifications_enabled'),
      isPermissionGranted(),
    ])
    setRows(reminders); setProjects(projectRows); setAccounts(accountRows); setTasks(taskRows); setEnabled(setting); setPermission(permitted)
  }, [db])

  // oxlint-disable-next-line react/set-state-in-effect -- Load persisted reminders when the page opens.
  useEffect(() => { void reload().catch(e => setError(errorMessage(e))) }, [reload])

  async function createReminder(event: FormEvent) {
    event.preventDefault(); setBusy(true); setError(''); setNotice('')
    try {
      const local = new Date(form.due_at)
      if (Number.isNaN(local.valueOf())) throw new Error('Choose a valid reminder date and time.')
      await invoke('save_reminder', { reminder: { id: newId(), title: form.title, body: form.body, kind: form.kind, dueAt: local.toISOString(), accountId: form.account_id || null, projectId: form.project_id || null, taskId: form.task_id || null } })
      setCreating(false); setForm(empty); setNotice('Reminder saved. It will be delivered when due while LEVI is running and notifications are enabled.'); await reload()
    } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }

  async function setState(row: Reminder, action: 'complete' | 'cancel' | 'snooze') {
    setError(''); setNotice('')
    try {
      const snoozeUntil = action === 'snooze' ? new Date(Date.now() + 60 * 60 * 1000).toISOString() : null
      await invoke('update_reminder_state', { reminderId: row.id, action, snoozeUntil })
      setNotice(action === 'snooze' ? `Snoozed “${row.title}” for one hour.` : action === 'complete' ? 'Reminder completed.' : 'Reminder cancelled.')
      await reload()
    } catch (e) { setError(errorMessage(e)) }
  }

  async function enableNotifications() {
    setError(''); setNotice('')
    try {
      const granted = permission || (await requestPermission()) === 'granted'
      setPermission(granted)
      if (!granted) throw new Error('Windows notification permission was not granted. Enable LEVI notifications in Windows Settings and retry.')
      await invoke('set_reminder_notifications_enabled', { enabled: true })
      setNotice('Desktop reminders are enabled. Due reminders will be checked every 30 seconds while LEVI is running.')
      await reload()
    } catch (e) { setError(errorMessage(e)) }
  }

  async function disableNotifications() {
    setError(''); setNotice('')
    try {
      await invoke('set_reminder_notifications_enabled', { enabled: false })
      setNotice('Desktop reminder delivery is disabled. Pending reminders remain saved.')
      await reload()
    } catch (e) { setError(errorMessage(e)) }
  }

  return <div className="feature-page workforce-page reminder-page">
    <div className="feature-header"><div><div className="eyebrow">CONTROL / PERSISTENT SCHEDULE</div><h1>Reminders</h1><p>Local reminders survive restarts. Delivery runs while LEVI is open.</p></div><div className="reminder-actions"><button className="button-secondary" onClick={() => void reload()}><RefreshCw size={14}/> Refresh</button><button className="button-primary" onClick={() => { setCreating(true); setError('') }}><Plus size={15}/> New reminder</button></div></div>
    {error && <div className="inline-error" role="alert">{error}</div>}{notice && <div className="inline-notice" role="status">{notice}</div>}
    <section className="reminder-notification panel"><div><Bell size={16}/><div><strong>{enabled ? 'Desktop notification delivery enabled' : 'Desktop notification delivery is off'}</strong><span>{enabled ? permission ? 'Due reminders are checked every 30 seconds while the application is running.' : 'Permission may be disabled in Windows Settings; delivery errors are retained.' : 'Enable Windows permission and opt in to due-time notifications.'}</span></div></div>{enabled ? <button className="button-secondary" onClick={() => void disableNotifications()}>Disable delivery</button> : <button className="button-secondary" onClick={() => void enableNotifications()}>Enable notifications</button>}</section>
    <div className="reminder-list">{rows.length ? rows.map(row => {
      const due = row.status === 'snoozed' ? row.snoozed_until : row.due_at
      return <article className="reminder-card panel" key={row.id}><div className="reminder-card-main"><span className={`work-status ${row.status}`}>{row.status}</span><h2>{row.title}</h2>{row.body && <p>{row.body}</p>}<div className="reminder-meta"><span><Clock3 size={12}/>{new Date(`${due?.replace(' ','T')}${due?.endsWith('Z') ? '' : 'Z'}`).toLocaleString()}</span>{row.account_name && <span>{row.account_name}</span>}{row.project_name && <span>{row.project_name}</span>}{row.task_title && <span>{row.task_title}</span>}{row.delivered_at && <span>Notification sent</span>}{row.delivery_attempts > 0 && <span>{row.delivery_attempts} delivery attempt{row.delivery_attempts === 1 ? '' : 's'}</span>}</div>{row.last_delivery_error && <div className="inline-error reminder-delivery-error">Notification delivery failed: {row.last_delivery_error}. LEVI will retry in five minutes.</div>}</div>{['pending','snoozed'].includes(row.status) && <div className="reminder-row-actions"><button className="button-secondary" onClick={() => void setState(row,'snooze')}><Clock3 size={13}/> Snooze 1h</button><button className="button-secondary" onClick={() => void setState(row,'complete')}><Check size={13}/> Complete</button><button className="card-icon-button" aria-label={`Cancel ${row.title}`} onClick={() => void setState(row,'cancel')}><X size={14}/></button></div>}</article>
    }) : <div className="empty-feature"><strong>No reminders yet</strong><p>Create a reminder or record a usage limit from AI Workforce Center.</p><button className="button-primary" onClick={() => setCreating(true)}><Plus size={14}/> Create your first reminder</button></div>}</div>
    {creating && <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget && !busy) setCreating(false) }}><form className="form-modal workforce-modal" onSubmit={createReminder}><div className="modal-heading"><div><span className="eyebrow">PERSISTENT REMINDER</span><h2>Create reminder</h2></div><button type="button" className="icon-button" onClick={() => setCreating(false)} aria-label="Close" disabled={busy}><X size={17}/></button></div><div className="modal-fields"><label className="wide-field">Title<input required maxLength={120} value={form.title} onChange={e => setForm({...form,title:e.target.value})}/></label><label className="wide-field">Details<textarea rows={3} maxLength={4000} value={form.body} onChange={e => setForm({...form,body:e.target.value})}/></label><label>Type<select value={form.kind} onChange={e => setForm({...form,kind:e.target.value})}><option value="one_time">One time</option><option value="deadline">Deadline</option><option value="check_in">Check-in</option></select></label><label>Due date and time<input required type="datetime-local" value={form.due_at} onChange={e => setForm({...form,due_at:e.target.value})}/></label><label>Account<select value={form.account_id} onChange={e => setForm({...form,account_id:e.target.value})}><option value="">No account</option>{accounts.map(x=><option key={x.id} value={x.id}>{x.name}</option>)}</select></label><label>Project<select value={form.project_id} onChange={e => setForm({...form,project_id:e.target.value})}><option value="">No project</option>{projects.map(x=><option key={x.id} value={x.id}>{x.name}</option>)}</select></label><label className="wide-field">Task<select value={form.task_id} onChange={e => setForm({...form,task_id:e.target.value})}><option value="">No task</option>{tasks.map(x=><option key={x.id} value={x.id}>{x.name}</option>)}</select></label></div>{error && <div className="inline-error" role="alert">{error}</div>}<p className="reminder-form-note">Recurring schedules are not supported yet. If LEVI is closed at the due time, the reminder remains pending and is checked after the next launch.</p><div className="modal-actions"><button type="button" className="button-secondary" onClick={() => setCreating(false)} disabled={busy}>Cancel</button><button className="button-primary" disabled={busy}>{busy ? 'Saving…' : 'Save reminder'}</button></div></form></div>}
  </div>
}
