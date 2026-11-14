import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type Database from '@tauri-apps/plugin-sql'
import { Activity, ArrowUpRight, Bell, CircleAlert, FolderKanban, ListTodo, UserRound, UsersRound } from 'lucide-react'
import { errorMessage } from '../../lib/database'
import './workforce.css'

interface Worker {
  id: string
  name: string
  account_status: string
  browser_name: string
  browser_status: string
  profile_name: string
  profile_status: string
  task_id: string | null
  task_title: string | null
  task_status: string | null
  project_name: string | null
  last_accessed_at: string | null
  limit_message: string | null
  reset_at: string | null
  reset_status: string | null
}
interface Summary { workers: number; available: number; active: number; waiting: number; review: number; blocked: number; queue: number }
interface Reminder { id: string; title: string; due_at: string; account_name: string | null; task_title: string | null }
interface EventRow { id: string; description: string; activity_type: string; occurred_at: string }
interface AssignmentRow { id: string; task_title: string; account_name: string; changed_at: string }
interface LimitForm { exact_message: string; reset_at: string; checkpoint_summary: string; remaining_work: string; files_changed: string; next_action: string }
const emptyLimit: LimitForm = { exact_message: '', reset_at: '', checkpoint_summary: '', remaining_work: '', files_changed: '', next_action: '' }

function workerState(worker: Worker): { label: string; tone: string } {
  if (worker.task_status === 'ready') return { label: 'Assigned', tone: 'working' }
  if (worker.task_status === 'review' || worker.account_status === 'needs_attention') return { label: 'In review', tone: 'review' }
  if (worker.task_status === 'blocked' || worker.account_status === 'blocked') return { label: 'Blocked', tone: 'blocked' }
  if (worker.account_status === 'waiting' || worker.task_status === 'waiting') return { label: 'Waiting', tone: 'waiting' }
  if (worker.account_status === 'working' && worker.task_status === 'in_progress') return { label: 'Working', tone: 'working' }
  if (worker.account_status === 'idle' && worker.browser_status === 'available' && worker.profile_status === 'available' && !worker.task_id) return { label: 'Available', tone: 'available' }
  return { label: 'Unavailable', tone: 'unavailable' }
}

export function WorkforceCenter({ db }: { db: Database }) {
  const [workers, setWorkers] = useState<Worker[]>([])
  const [summary, setSummary] = useState<Summary | null>(null)
  const [reminders, setReminders] = useState<Reminder[]>([])
  const [events, setEvents] = useState<EventRow[]>([])
  const [assignmentFailures, setAssignmentFailures] = useState<EventRow[]>([])
  const [recentAssignments, setRecentAssignments] = useState<AssignmentRow[]>([])
  const [fileConflictCount, setFileConflictCount] = useState(0)
  const [limitTarget, setLimitTarget] = useState<Worker | null>(null)
  const [limitForm, setLimitForm] = useState<LimitForm>(emptyLimit)
  const [limitError, setLimitError] = useState('')
  const [savingLimit, setSavingLimit] = useState(false)
  const [resumeTarget, setResumeTarget] = useState<Worker | null>(null)
  const [resetVerified, setResetVerified] = useState(false)
  const [resumeAuthorized, setResumeAuthorized] = useState(false)
  const [resumeError, setResumeError] = useState('')
  const [savingResume, setSavingResume] = useState(false)
  const [error, setError] = useState('')
  const reload = useCallback(async () => {
    const [workerRows, taskCounts, remindersRows, eventRows, assignmentRows, conflictRows, failureRows] = await Promise.all([
      db.select<Worker[]>(`SELECT a.id,a.name,a.status AS account_status,b.name AS browser_name,b.availability_status AS browser_status,
          bp.profile_name,bp.availability_status AS profile_status,t.id AS task_id,t.title AS task_title,t.status AS task_status,
          p.name AS project_name,a.last_accessed_at,
          (SELECT u.exact_message FROM usage_limit_events u WHERE u.account_id=a.id AND u.status='waiting' ORDER BY u.detected_at DESC LIMIT 1) AS limit_message,
          (SELECT u.reset_at FROM usage_limit_events u WHERE u.account_id=a.id AND u.status='waiting' ORDER BY u.detected_at DESC LIMIT 1) AS reset_at,
          (SELECT u.reset_status FROM usage_limit_events u WHERE u.account_id=a.id AND u.status='waiting' ORDER BY u.detected_at DESC LIMIT 1) AS reset_status
        FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id
        LEFT JOIN tasks t ON t.id=(SELECT t2.id FROM tasks t2 WHERE t2.account_id=a.id AND t2.archived_at IS NULL AND t2.status IN ('ready','in_progress','waiting','review','blocked') ORDER BY CASE t2.status WHEN 'in_progress' THEN 0 WHEN 'ready' THEN 1 WHEN 'waiting' THEN 2 WHEN 'review' THEN 3 ELSE 4 END,t2.created_at LIMIT 1)
        LEFT JOIN projects p ON p.id=t.project_id WHERE a.archived_at IS NULL ORDER BY a.priority DESC,a.name COLLATE NOCASE`),
      db.select<{ waiting: number; review: number; blocked: number; queue: number; active: number }[]>(`SELECT
        SUM(CASE WHEN status='waiting' THEN 1 ELSE 0 END) waiting,
        SUM(CASE WHEN status='review' THEN 1 ELSE 0 END) review,
        SUM(CASE WHEN status='blocked' THEN 1 ELSE 0 END) blocked,
        SUM(CASE WHEN status IN ('backlog','ready') AND account_id IS NULL THEN 1 ELSE 0 END) queue,
        SUM(CASE WHEN status IN ('ready','in_progress') THEN 1 ELSE 0 END) active
        FROM tasks WHERE archived_at IS NULL`),
      db.select<Reminder[]>(`SELECT r.id,r.title,r.due_at,a.name AS account_name,t.title AS task_title FROM reminders r
        LEFT JOIN accounts a ON a.id=r.account_id LEFT JOIN tasks t ON t.id=r.task_id
        WHERE r.status IN ('pending','snoozed') AND COALESCE(r.snoozed_until,r.due_at)>=datetime('now')
        ORDER BY COALESCE(r.snoozed_until,r.due_at) LIMIT 5`),
      db.select<EventRow[]>(`SELECT id,description,activity_type,occurred_at FROM activity_logs ORDER BY occurred_at DESC LIMIT 6`),
      db.select<AssignmentRow[]>(`SELECT h.id,t.title AS task_title,COALESCE(a.name,'Unassigned') AS account_name,h.changed_at FROM task_history h JOIN tasks t ON t.id=h.task_id LEFT JOIN accounts a ON a.id=h.new_value WHERE h.field='account_id' AND h.new_value IS NOT NULL ORDER BY h.changed_at DESC LIMIT 5`),
      db.select<{ count:number }[]>(`SELECT COUNT(*) AS count FROM task_file_claims x JOIN task_file_claims y ON x.project_id=y.project_id AND x.task_id<y.task_id AND x.status='active' AND y.status='active' AND (lower(x.path_key)=lower(y.path_key) OR substr(lower(x.path_key),1,length(y.path_key)+1)=lower(y.path_key)||'/' OR substr(lower(y.path_key),1,length(x.path_key)+1)=lower(x.path_key)||'/')`),
      db.select<EventRow[]>(`SELECT id,description,activity_type,occurred_at FROM activity_logs WHERE activity_type='task_assignment_failed' ORDER BY occurred_at DESC LIMIT 5`),
    ])
    const counts = taskCounts[0]
    const states = workerRows.map(workerState)
    setWorkers(workerRows)
    setSummary({ workers: workerRows.length, available: states.filter(s => s.label === 'Available').length, active: counts?.active ?? 0, waiting: counts?.waiting ?? 0, review: counts?.review ?? 0, blocked: counts?.blocked ?? 0, queue: counts?.queue ?? 0 })
    setReminders(remindersRows)
    setEvents(eventRows)
    setRecentAssignments(assignmentRows)
    setFileConflictCount(conflictRows[0]?.count ?? 0)
    setAssignmentFailures(failureRows)
    setError('')
  }, [db])
  // oxlint-disable-next-line react/set-state-in-effect -- Populate the workforce view from authoritative local records.
  useEffect(() => { void reload().catch(e => setError(errorMessage(e))) }, [reload])
  useEffect(() => {
    const onAssignment = () => { void reload().catch(e => setError(errorMessage(e))) }
    window.addEventListener('levi:queue-assignment', onAssignment)
    return () => window.removeEventListener('levi:queue-assignment', onAssignment)
  }, [reload])

  async function submitUsageLimit(event: FormEvent) {
    event.preventDefault()
    if (!limitTarget) return
    setSavingLimit(true); setLimitError('')
    try {
      const resetAt = limitForm.reset_at ? new Date(limitForm.reset_at).toISOString().replace('T', ' ').slice(0, 19) : null
      await invoke('mark_usage_limited', { input: {
        accountId: limitTarget.id,
        exactMessage: limitForm.exact_message,
        resetAt,
        checkpointSummary: limitForm.checkpoint_summary,
        remainingWork: limitForm.remaining_work,
        filesChanged: limitForm.files_changed,
        lastSuccessfulStep: '',
        nextAction: limitForm.next_action,
        blockers: '',
      } })
      setLimitTarget(null); setLimitForm(emptyLimit); await reload()
    } catch (e) { setLimitError(errorMessage(e)) } finally { setSavingLimit(false) }
  }

  async function submitResume(event: FormEvent) {
    event.preventDefault()
    if (!resumeTarget) return
    setSavingResume(true); setResumeError('')
    try {
      await invoke('resume_usage_limited', { input: { accountId: resumeTarget.id, resetVerified, userAuthorized: resumeAuthorized } })
      setResumeTarget(null); setResetVerified(false); setResumeAuthorized(false); await reload()
    } catch (e) { setResumeError(errorMessage(e)) } finally { setSavingResume(false) }
  }

  return <div className="feature-page workforce-page command-page">
    <div className="feature-header"><div><div className="eyebrow"><span className="live-pulse"/> OPERATIONS / WORKFORCE</div><h1>AI Workforce Center</h1><p>Operational status from registered accounts, tasks, reminders and activity records.</p></div><button className="button-secondary" onClick={() => void reload()}><Activity size={14}/> Refresh</button></div>
    {error && <div className="inline-error" role="alert">Workforce records could not be loaded: {error}</div>}
    <section className="command-metrics" aria-label="Workforce summary">
      {[
        { name: 'Registered workers', value: summary?.workers, icon: UsersRound },
        { name: 'Available', value: summary?.available, icon: UserRound },
        { name: 'Active task records', value: summary?.active, icon: ListTodo },
        { name: 'Unassigned queue', value: summary?.queue, icon: FolderKanban },
        { name: 'Active file conflicts', value: fileConflictCount, icon: CircleAlert },
        { name: 'Waiting / review / blocked', value: summary ? `${summary.waiting} / ${summary.review} / ${summary.blocked}` : undefined, icon: CircleAlert },
      ].map(item => <article className="command-metric" key={item.name}><div><item.icon size={15}/><span>{item.name}</span></div><strong>{item.value ?? '—'}</strong></article>)}
    </section>
    <div className="command-columns">
      <section className="panel command-workers"><div className="panel-heading"><div><h2>Registered workers</h2><p>Availability is based on saved account and browser state, not process presence.</p></div><button className="text-button" onClick={() => window.dispatchEvent(new CustomEvent('navigate', { detail: 'Accounts' }))}>Manage accounts <ArrowUpRight size={13}/></button></div>
        {workers.length ? <div className="worker-table">{workers.map(worker => { const state = workerState(worker); return <article className="worker-row" key={worker.id}>
          <div className="worker-avatar"><UserRound size={16}/></div><div className="worker-identity"><strong>{worker.name}</strong><span>{worker.browser_name} · {worker.profile_name}</span></div>
          <span className={`worker-state ${state.tone}`}>{state.label}</span><div className="worker-task"><strong>{worker.task_title ?? 'No assigned active task'}</strong><span>{[worker.project_name, worker.task_status?.replaceAll('_', ' ')].filter(Boolean).join(' · ') || `${worker.browser_status} browser · ${worker.profile_status} profile`}</span>{worker.limit_message && <small className="worker-limit-detail" title={worker.limit_message}>{worker.reset_status === 'confirmed' ? `Reset reported ${worker.reset_at}` : 'Reset time unknown'} · ${worker.limit_message}</small>}</div>
          <button className="usage-limit-button" onClick={() => { setLimitTarget(worker); setLimitForm(emptyLimit); setLimitError('') }}>Mark usage limited</button>
          {worker.limit_message && state.label === 'Waiting' && <button className="resume-worker-button" onClick={() => { setResumeTarget(worker); setResetVerified(false); setResumeAuthorized(false); setResumeError('') }}>Resume same account</button>}
        </article> })}</div> : <div className="command-empty">No registered accounts are available to display.</div>}
        <div className="command-footnote">“Working” reflects the account’s explicitly saved status and an in-progress task. Browser launch alone is not treated as work activity.</div>
      </section>
      <div className="command-side-stack">
        <section className="panel command-side-panel"><div className="panel-heading"><div><h2>Upcoming reminders</h2><p>Persisted reminders due next</p></div><Bell size={15}/></div>{reminders.length ? <div className="command-list">{reminders.map(item => <div className="command-list-row" key={item.id}><strong>{item.title}</strong><span>{item.account_name ?? item.task_title ?? 'Workspace reminder'} · {item.due_at}</span></div>)}</div> : <div className="command-empty compact">No upcoming reminders recorded.</div>}</section>
        <section className="panel command-side-panel"><div className="panel-heading"><div><h2>Recent assignments</h2><p>Committed task assignment history</p></div><ListTodo size={15}/></div>{recentAssignments.length ? <div className="command-list">{recentAssignments.map(item => <div className="command-list-row" key={item.id}><strong>{item.task_title} → {item.account_name}</strong><span>{item.changed_at}</span></div>)}</div> : <div className="command-empty compact">No task assignments have been recorded.</div>}</section>
        <section className="panel command-side-panel"><div className="panel-heading"><div><h2>Assignment failures</h2><p>Recent rejected assignment attempts</p></div><CircleAlert size={15}/></div>{assignmentFailures.length ? <div className="command-list">{assignmentFailures.map(item => <div className="command-list-row" key={item.id}><strong>{item.description}</strong><span>{item.occurred_at}</span></div>)}</div> : <div className="command-empty compact">No assignment failures recorded.</div>}</section>
        <section className="panel command-side-panel"><div className="panel-heading"><div><h2>Operational history</h2><p>Latest saved activity events</p></div><Activity size={15}/></div>{events.length ? <div className="command-list">{events.map(item => <div className="command-list-row" key={item.id}><strong>{item.description}</strong><span>{item.activity_type.replaceAll('_', ' ')} · {item.occurred_at}</span></div>)}</div> : <div className="command-empty compact">No activity has been recorded yet.</div>}</section>
      </div>
    </div>
    <div className="command-disclosure">Task assignment is transactional and does not mean work has started. Dependencies and project-scoped file claims are enforced by Rust. Claude usage-limit detection remains manual; automatic browser-page monitoring is not implemented.</div>
    {limitTarget && <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget && !savingLimit) setLimitTarget(null) }}><form className="form-modal workforce-modal" onSubmit={submitUsageLimit}>
      <div className="modal-heading"><div><span className="eyebrow">MANUAL REPORT</span><h2>Mark usage limited</h2><p className="subtitle">{limitTarget.name}{limitTarget.task_title ? ` · ${limitTarget.task_title}` : ''}</p></div><button type="button" className="icon-button" disabled={savingLimit} onClick={() => setLimitTarget(null)} aria-label="Close">×</button></div>
      <div className="modal-fields"><label className="wide-field">Exact usage-limit message<textarea required maxLength={4000} rows={3} value={limitForm.exact_message} onChange={e => setLimitForm({ ...limitForm, exact_message: e.target.value })} placeholder="Enter the message displayed by Claude; it will be stored as provided."/></label><label>Reset time <small>Leave blank if not shown.</small><input type="datetime-local" value={limitForm.reset_at} onChange={e => setLimitForm({ ...limitForm, reset_at: e.target.value })}/></label><label className="wide-field">Checkpoint summary<textarea rows={2} value={limitForm.checkpoint_summary} onChange={e => setLimitForm({ ...limitForm, checkpoint_summary: e.target.value })} placeholder="Completed work to preserve"/></label><label className="wide-field">Remaining work<textarea rows={2} value={limitForm.remaining_work} onChange={e => setLimitForm({ ...limitForm, remaining_work: e.target.value })} placeholder="What still needs to be done"/></label><label className="wide-field">Files changed<input value={limitForm.files_changed} onChange={e => setLimitForm({ ...limitForm, files_changed: e.target.value })} placeholder="Optional file paths"/></label><label className="wide-field">Next action<input value={limitForm.next_action} onChange={e => setLimitForm({ ...limitForm, next_action: e.target.value })} placeholder="Optional continuation step"/></label></div>
      {limitError && <div className="inline-error" role="alert">{limitError}</div>}<p className="usage-limit-note">This user-reported action stores a checkpoint, moves the current task and account to Waiting, and creates a persistent reminder. It does not verify a reset or resume work automatically.</p><div className="modal-actions"><button type="button" className="button-secondary" onClick={() => setLimitTarget(null)} disabled={savingLimit}>Cancel</button><button className="button-primary" disabled={savingLimit}>{savingLimit ? 'Saving…' : 'Record usage limit'}</button></div>
    </form></div>}
    {resumeTarget && <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget && !savingResume) setResumeTarget(null) }}><form className="form-modal workforce-modal" onSubmit={submitResume}>
      <div className="modal-heading"><div><span className="eyebrow">SAME ACCOUNT RESUME</span><h2>Resume waiting work</h2><p className="subtitle">{resumeTarget.name}{resumeTarget.task_title ? ` · ${resumeTarget.task_title}` : ''}</p></div><button type="button" className="icon-button" disabled={savingResume} onClick={() => setResumeTarget(null)} aria-label="Close">×</button></div>
      <p className="resume-warning">LEVI cannot verify Claude’s limit state. Resume only after you have checked the account yourself. This returns the original task to Ready under the same account; it does not rotate workers.</p>
      <label className="resume-confirm"><input type="checkbox" checked={resetVerified} onChange={e => setResetVerified(e.target.checked)}/> I verified this account is available to continue.</label>
      <label className="resume-confirm"><input type="checkbox" checked={resumeAuthorized} onChange={e => setResumeAuthorized(e.target.checked)}/> I authorize resuming this task on the same account.</label>
      {resumeError && <div className="inline-error" role="alert">{resumeError}</div>}<div className="modal-actions"><button type="button" className="button-secondary" disabled={savingResume} onClick={() => setResumeTarget(null)}>Cancel</button><button className="button-primary" disabled={savingResume || !resetVerified || !resumeAuthorized}>{savingResume ? 'Resuming…' : 'Resume same account'}</button></div>
    </form></div>}
  </div>
}
