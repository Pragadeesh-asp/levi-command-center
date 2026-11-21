import { useCallback, useEffect, useMemo, useState, type FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type Database from '@tauri-apps/plugin-sql'
import { Archive, Pencil, Plus, Search, X } from 'lucide-react'
import { errorMessage, newId } from '../../lib/database'
import { SpringCheck } from './SpringCheck'
import { MagicBentoCard } from './MagicBentoCard'
import './workforce.css'

interface Task { id: string; project_id: string | null; project_name: string | null; account_id: string | null; account_name: string | null; title: string; description: string; priority: number; status: string; deadline: string | null; expected_minutes: number | null; instructions: string; completion_notes: string; blocker: string; dependencies: { id: string; title: string; status: string; archived_at: string | null }[]; intendedFiles: string[]; modifiedFiles: string[]; releasedFiles: string[]; conflicts: string[]; assignments: string[] }
interface Option { id: string; name: string; status?: string; browser_status?: string; profile_status?: string; active_task?: string | null }
interface Form { project_id: string; account_id: string; title: string; description: string; priority: number; status: string; deadline: string; expected_minutes: string; instructions: string; completion_notes: string; blocker: string; depends_on: string[]; intended_files: string; modified_files: string; released_files: string }
const empty: Form = { project_id: '', account_id: '', title: '', description: '', priority: 2, status: 'ready', deadline: '', expected_minutes: '', instructions: '', completion_notes: '', blocker: '', depends_on: [], intended_files: '', modified_files: '', released_files: '' }
const statuses = ['backlog', 'ready', 'in_progress', 'waiting', 'review', 'blocked', 'completed']
const priorities = [{ value: 4, label: 'P4 · Urgent' }, { value: 3, label: 'P3 · High' }, { value: 2, label: 'P2 · Normal' }, { value: 1, label: 'P1 · Low' }]
const label = (v: string) => v.replaceAll('_', ' ').replace(/\b\w/g, c => c.toUpperCase())
function dependencyChain(tasks: Task[], taskId: string, visited = new Set<string>()): Task['dependencies'] {
  if (visited.has(taskId)) return []
  visited.add(taskId)
  const task = tasks.find(item => item.id === taskId)
  if (!task) return []
  return task.dependencies.flatMap(dependency => [dependency, ...dependencyChain(tasks, dependency.id, visited)])
}
function queueStatus(task: Task, tasks: Task[]): string {
  const unresolved = dependencyChain(tasks, task.id).some(dependency => dependency.status !== 'completed' || dependency.archived_at)
  return task.status === 'ready' && (unresolved || task.conflicts.length > 0 || Boolean(task.blocker.trim())) ? 'blocked' : task.status
}

export function TasksPage({ db }: { db: Database }) {
  const [tasks, setTasks] = useState<Task[]>([])
  const [projects, setProjects] = useState<Option[]>([])
  const [accounts, setAccounts] = useState<Option[]>([])
  const [editor, setEditor] = useState<Task | null | false>(false)
  const [form, setForm] = useState<Form>(empty)
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState('all')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)
  const [autoAssign, setAutoAssign] = useState(false)
  const [assigning, setAssigning] = useState('')
  const [workerChoice, setWorkerChoice] = useState<Record<string, string>>({})
  const [completionPending, setCompletionPending] = useState<string | null>(null)
  const reload = useCallback(async () => {
    const [taskRows, projectRows, accountRows, dependencyRows, fileRows, releaseRows, activeClaims, historyRows] = await Promise.all([
      db.select<Task[]>(`SELECT t.id,t.project_id,p.name AS project_name,t.account_id,a.name AS account_name,t.title,t.description,t.priority,t.status,t.deadline,t.expected_minutes,t.instructions,t.completion_notes,t.blocker
        FROM tasks t LEFT JOIN projects p ON p.id=t.project_id LEFT JOIN accounts a ON a.id=t.account_id WHERE t.archived_at IS NULL
        ORDER BY CASE t.status WHEN 'in_progress' THEN 0 WHEN 'ready' THEN 1 WHEN 'backlog' THEN 2 WHEN 'waiting' THEN 3 WHEN 'review' THEN 4 WHEN 'blocked' THEN 5 ELSE 6 END,t.priority DESC,COALESCE(t.deadline,'9999-12-31'),t.created_at`),
      db.select<Option[]>("SELECT id,name FROM projects WHERE archived_at IS NULL AND status='active' ORDER BY name COLLATE NOCASE"),
      db.select<Option[]>(`SELECT a.id,a.name,a.status,b.availability_status AS browser_status,bp.availability_status AS profile_status,
          (SELECT t.title FROM tasks t WHERE t.account_id=a.id AND t.archived_at IS NULL AND t.status IN ('ready','in_progress','waiting','review','blocked') ORDER BY t.created_at LIMIT 1) AS active_task
        FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id
        WHERE a.archived_at IS NULL AND bp.archived_at IS NULL ORDER BY a.name COLLATE NOCASE`),
      db.select<{ task_id:string; id:string; title:string; status:string; archived_at:string|null }[]>(`SELECT d.task_id,t.id,t.title,t.status,t.archived_at FROM task_dependencies d JOIN tasks t ON t.id=d.depends_on_task_id`),
      db.select<{ task_id:string; path_display:string; claim_kind:string }[]>(`SELECT task_id,path_display,claim_kind FROM task_file_declarations`),
      db.select<{ task_id:string; path_display:string }[]>(`SELECT task_id,path_display FROM task_file_releases ORDER BY released_at DESC`),
      db.select<{ task_id:string; path_display:string }[]>(`SELECT task_id,path_display FROM task_file_claims WHERE status='active'`),
      db.select<{ task_id:string; new_value:string|null; changed_at:string }[]>(`SELECT task_id,new_value,changed_at FROM task_history WHERE field='account_id' ORDER BY changed_at DESC`),
    ])
    const enriched = taskRows.map(task => {
      const dependencies = dependencyRows.filter(item => item.task_id === task.id).map(({ id,title,status,archived_at }) => ({ id,title,status,archived_at }))
      const intendedFiles = fileRows.filter(item => item.task_id === task.id && item.claim_kind === 'intended').map(item => item.path_display)
      const modifiedFiles = fileRows.filter(item => item.task_id === task.id && item.claim_kind === 'modified').map(item => item.path_display)
      const releasedFiles = [...new Set(releaseRows.filter(item => item.task_id === task.id).map(item => item.path_display))]
      const conflicts = [...new Set([...intendedFiles,...modifiedFiles].filter(path => activeClaims.some(claim => claim.task_id !== task.id && taskRows.find(other => other.id === claim.task_id)?.project_id === task.project_id && (claim.path_display.toLowerCase() === path.toLowerCase() || claim.path_display.toLowerCase().startsWith(`${path.toLowerCase()}/`) || path.toLowerCase().startsWith(`${claim.path_display.toLowerCase()}/`)))))]
      const assignments = historyRows.filter(row => row.task_id === task.id && row.new_value).map(row => `${row.changed_at}: ${accountRows.find(account => account.id === row.new_value)?.name ?? 'Account assignment changed'}`)
      return { ...task, dependencies, intendedFiles, modifiedFiles, releasedFiles, conflicts, assignments }
    })
    setTasks(enriched); setProjects(projectRows); setAccounts(accountRows)
  }, [db])
  // oxlint-disable-next-line react/set-state-in-effect -- Load persisted task and assignment records when this page mounts.
  useEffect(() => { void reload().catch(e => setError(errorMessage(e))); void invoke<boolean>('get_auto_assignment').then(setAutoAssign).catch(e => setError(errorMessage(e))) }, [reload])
  useEffect(() => {
    const onAssignment = (event: Event) => { const result = (event as CustomEvent<{taskTitle:string;accountName:string}>).detail; setNotice(`Automatically assigned “${result.taskTitle}” to ${result.accountName}. Work has not been started.`); void reload() }
    const onQueueError = (event: Event) => setError(`Automatic assignment failed: ${(event as CustomEvent<string>).detail}`)
    window.addEventListener('levi:queue-assignment', onAssignment); window.addEventListener('levi:queue-error', onQueueError)
    return () => { window.removeEventListener('levi:queue-assignment', onAssignment); window.removeEventListener('levi:queue-error', onQueueError) }
  }, [reload])
  const visible = useMemo(() => tasks.filter(task => (statusFilter === 'all' || queueStatus(task, tasks) === statusFilter) && `${task.title} ${task.project_name ?? ''} ${task.account_name ?? ''} ${task.description}`.toLowerCase().includes(search.toLowerCase())), [tasks, statusFilter, search])
  const availabilityReason = (account: Option) => account.status !== 'idle' ? `Account is ${label(account.status ?? 'unknown')}` : account.browser_status !== 'available' ? `Browser is ${label(account.browser_status ?? 'unknown')}` : account.profile_status !== 'available' ? `Profile is ${label(account.profile_status ?? 'unknown')}` : account.active_task ? `Assigned to ${account.active_task}` : ''
  function create() { setEditor(null); setForm(empty); setError('') }
  function edit(task: Task) { setEditor(task); setForm({ project_id: task.project_id ?? '', account_id: task.account_id ?? '', title: task.title, description: task.description, priority: task.priority, status: task.status, deadline: task.deadline ?? '', expected_minutes: task.expected_minutes == null ? '' : String(task.expected_minutes), instructions: task.instructions, completion_notes: task.completion_notes, blocker: task.blocker, depends_on: task.dependencies.map(item => item.id), intended_files: task.intendedFiles.join('\n'), modified_files: task.modifiedFiles.join('\n'), released_files: '' }); setError('') }
  async function save(event: FormEvent) {
    event.preventDefault(); setError(''); setBusy(true)
    try {
      await invoke('save_task', { task: { id: editor ? editor.id : newId(), projectId: form.project_id || null, accountId: form.account_id || null, title: form.title, description: form.description, priority: form.priority, status: form.status, deadline: form.deadline || null, expectedMinutes: form.expected_minutes === '' ? null : Number(form.expected_minutes), instructions: form.instructions, completionNotes: form.completion_notes, blocker: form.blocker, dependsOn: form.depends_on, intendedFiles: form.intended_files, modifiedFiles: form.modified_files, releasedFiles: form.released_files }, isUpdate: Boolean(editor) })
      setEditor(false); setNotice(editor ? 'Task updated and history recorded.' : 'Task created and history recorded.'); await reload()
    } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }
  async function archive(task: Task) {
    if (!window.confirm(`Archive “${task.title}”? Its history will be preserved.`)) return
    setError(''); setNotice('')
    try { await invoke('archive_task', { taskId: task.id }); setNotice('Task archived.'); await reload() }
    catch (e) { setError(errorMessage(e)) }
  }

  async function toggleCompletion(task: Task, checked: boolean) {
    if (completionPending) return
    setCompletionPending(task.id); setError(''); setNotice('')
    try {
      await invoke('save_task', { task: {
        id: task.id, projectId: task.project_id, accountId: task.account_id, title: task.title,
        description: task.description, priority: task.priority, status: checked ? 'completed' : 'ready',
        deadline: task.deadline, expectedMinutes: task.expected_minutes, instructions: task.instructions,
        completionNotes: task.completion_notes, blocker: task.blocker, dependsOn: task.dependencies.map(item => item.id),
        intendedFiles: task.intendedFiles.join('\n'), modifiedFiles: task.modifiedFiles.join('\n'), releasedFiles: '',
      }, isUpdate: true })
      setNotice(checked ? `“${task.title}” marked complete.` : `“${task.title}” reopened as Ready.`)
      await reload()
    } catch (e) { setError(errorMessage(e)) }
    finally { setCompletionPending(null) }
  }
  async function setAuto(value: boolean) { setError(''); try { await invoke('set_auto_assignment', { enabled: value }); setAutoAssign(value); setNotice(value ? 'Automatic assignment enabled. The queue checks every 10 seconds while this screen is open.' : 'Automatic assignment disabled.') } catch (e) { setError(errorMessage(e)) } }
  async function assignNext() { setError(''); setAssigning('next'); try { const result = await invoke<{taskTitle:string;accountName:string}|null>('assign_next_eligible'); setNotice(result ? `Assigned “${result.taskTitle}” to ${result.accountName}. Assignment is recorded; work has not been started.` : 'No eligible Ready task and available worker were found.'); await reload() } catch (e) { setError(errorMessage(e)) } finally { setAssigning('') } }
  async function assign(task: Task) { const accountId = workerChoice[task.id]; if (!accountId) return; setError(''); setAssigning(task.id); try { const result = await invoke<{taskTitle:string;accountName:string}>('assign_task', { taskId: task.id, accountId }); setNotice(`Assigned “${result.taskTitle}” to ${result.accountName}. Work has not been started.`); await reload() } catch (e) { setError(errorMessage(e)) } finally { setAssigning('') } }
  async function release(task: Task) { if (!window.confirm(`Release “${task.title}” from its worker? Its active file claims will be released and the task returned to Ready.`)) return; setError(''); try { await invoke('release_task_assignment', { taskId: task.id }); setNotice('Assignment and file claims released.'); await reload() } catch (e) { setError(errorMessage(e)) } }
  return <div className="feature-page workforce-page">
    <div className="feature-header"><div><div className="eyebrow">OPERATIONS / TASKS</div><h1>Task queue</h1><p>Assignment is recorded only after Rust validates and commits it. Assignment does not mean work has started.</p></div><div className="queue-header-actions"><label className="auto-assignment-toggle"><input type="checkbox" checked={autoAssign} onChange={e => void setAuto(e.target.checked)}/> Automatic assignment</label><button className="button-secondary" disabled={assigning==='next'} onClick={() => void assignNext()}>{assigning==='next' ? 'Checking…' : 'Assign next eligible'}</button><button className="button-primary" onClick={create}><Plus size={16}/> New task</button></div></div>
    {notice && <div className="inline-notice" role="status">{notice}</div>}{error && !editor && <div className="inline-error" role="alert">{error}</div>}
    <div className="task-toolbar"><label className="searchbox"><Search size={15}/><input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search tasks, projects, accounts"/></label><select value={statusFilter} onChange={e => setStatusFilter(e.target.value)}><option value="all">All statuses</option>{statuses.map(s => <option key={s} value={s}>{label(s)}</option>)}</select><span className="result-count">{visible.length} {visible.length === 1 ? 'task' : 'tasks'}</span></div>
    <div className="queue-summary">{['ready','in_progress','blocked','waiting','review'].map(status => <button key={status} className={statusFilter===status?'selected':''} onClick={() => setStatusFilter(status)}><strong>{tasks.filter(t => queueStatus(t,tasks)===status).length}</strong><span>{label(status)}</span></button>)}<button onClick={() => void reload()}><strong>↻</strong><span>Refresh queue</span></button></div>
    {visible.length ? <div className="task-list">{visible.map(task => { const chain = dependencyChain(tasks,task.id); const queueState = queueStatus(task,tasks); const unfinished = chain.filter(dep => dep.status !== 'completed' || dep.archived_at); const unassigned = !task.account_id && task.status === 'ready'; return <MagicBentoCard className="task-card queue-task-card" key={task.id}><div className="task-card-main"><div className="task-title-line"><SpringCheck checked={task.status==="completed"} disabled={completionPending!==null} label={`${task.status==="completed"?"Reopen":"Complete"} task: ${task.title}`} onChange={checked=>void toggleCompletion(task,checked)}/><span className={`work-status ${queueState}`}>{label(queueState)}</span><span className="priority-label">P{task.priority}</span><h2>{task.title}</h2></div><p>{task.description || 'No description.'}</p><div className="task-meta"><span>{task.project_name || 'No project'}</span><span>{task.account_name ? `Assigned to ${task.account_name}` : 'Unassigned'}</span>{task.deadline && <span>Due {task.deadline}</span>}{task.expected_minutes != null && <span>{task.expected_minutes} min estimate</span>}{unfinished.length > 0 && <span className="task-blocker">Blocked by: {unfinished.map(d => `${d.title}${d.archived_at?' (archived)':` (${label(d.status)})`}`).join(', ')}</span>}{task.conflicts.length > 0 && <span className="task-blocker">File conflict: {task.conflicts.join(', ')}</span>}{task.blocker && <span className="task-blocker">Blocker: {task.blocker}</span>}</div><details className="queue-details"><summary>Dependencies, file ownership and assignment history</summary><div><strong>Dependencies</strong><p>{chain.length ? chain.map(dep => `${dep.title} (${dep.archived_at?'Archived':label(dep.status)})`).join(' → ') : 'None'}</p><strong>Declared files</strong><p>{[...task.intendedFiles.map(f => `Intent: ${f}`),...task.modifiedFiles.map(f => `Modified: ${f}`)].join(', ') || 'None'}</p><strong>Released files</strong><p>{task.releasedFiles.join(', ') || 'None recorded'}</p><strong>Assignment history</strong><p>{task.assignments.slice(0,5).join(' · ') || 'No assignment history recorded.'}</p></div></details></div><div className="work-card-actions"><button className="button-secondary" onClick={() => edit(task)}><Pencil size={13}/> Edit</button>{unassigned && <><select aria-label={`Choose worker for ${task.title}`} value={workerChoice[task.id] ?? ''} onChange={e => setWorkerChoice({ ...workerChoice, [task.id]:e.target.value })}><option value="">Select worker</option>{accounts.map(account => <option key={account.id} value={account.id} disabled={Boolean(availabilityReason(account)) || !projects.some(p=>p.id===task.project_id)}>{account.name} · {availabilityReason(account) || (projects.some(p=>p.id===task.project_id)?'Available':'Assign to project first')}</option>)}</select><button className="button-secondary" disabled={!workerChoice[task.id] || assigning===task.id || unfinished.length>0 || task.conflicts.length>0} onClick={() => void assign(task)}>{assigning===task.id?'Assigning…':'Assign'}</button></>}{task.account_id && <button className="button-secondary" onClick={() => void release(task)}>Release</button>}<button className="card-icon-button archive-action" onClick={() => void archive(task)} aria-label={`Archive ${task.title}`} title="Archive task"><Archive size={14}/></button></div></MagicBentoCard> })}</div> : <div className="empty-feature"><strong>{tasks.length ? 'No tasks match these filters' : 'No tasks yet'}</strong><p>Create a task, connect it to an active project, and define dependencies or file ownership.</p>{!tasks.length && <button className="button-primary" onClick={create}><Plus size={14}/> Create your first task</button>}</div>}
    {editor !== false && <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) setEditor(false) }}><form className="form-modal workforce-modal" onSubmit={save}>
      <div className="modal-heading"><div><span className="eyebrow">WORK ITEM</span><h2>{editor ? 'Edit task' : 'Create task'}</h2></div><button type="button" className="icon-button" onClick={() => setEditor(false)} aria-label="Close"><X size={17}/></button></div>
      <div className="modal-fields"><label className="wide-field">Task title<input autoFocus required maxLength={180} value={form.title} onChange={e => setForm({ ...form, title: e.target.value })}/></label><label className="wide-field">Description<textarea rows={3} maxLength={8000} value={form.description} onChange={e => setForm({ ...form, description: e.target.value })}/></label><label>Project<select value={form.project_id} onChange={e => setForm({ ...form, project_id: e.target.value, account_id: '', depends_on: [] })}><option value="">No project</option>{projects.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select></label><label>Assigned account<select value={form.account_id} onChange={e => setForm({ ...form, account_id: e.target.value })}><option value="">Unassigned</option>{accounts.map(a => { const reason = availabilityReason(a); return <option key={a.id} value={a.id} disabled={Boolean(reason) && a.id !== form.account_id}>{a.name} · {reason || 'Available'}</option> })}</select><small>Assignment is validated again inside the Rust transaction. An assigned task is not evidence that the worker has started.</small></label><label>Status<select value={form.status} onChange={e => setForm({ ...form, status: e.target.value })}>{statuses.map(s => <option key={s} value={s}>{label(s)}</option>)}</select></label><label>Priority<select value={form.priority} onChange={e => setForm({ ...form, priority: Number(e.target.value) })}>{priorities.map(p => <option key={p.value} value={p.value}>{p.label}</option>)}</select></label><label>Deadline<input type="date" value={form.deadline} onChange={e => setForm({ ...form, deadline: e.target.value })}/></label><label>Estimated minutes<input type="number" min="0" max="100000" value={form.expected_minutes} onChange={e => setForm({ ...form, expected_minutes: e.target.value })}/></label><fieldset className="wide-field dependency-picker"><legend>Depends on (same project)</legend>{tasks.filter(t => t.id !== (editor ? editor.id : '') && t.project_id === form.project_id).map(task => <label key={task.id}><input type="checkbox" checked={form.depends_on.includes(task.id)} onChange={e => setForm({ ...form, depends_on: e.target.checked ? [...form.depends_on,task.id] : form.depends_on.filter(id=>id!==task.id) })}/>{task.title} · {label(task.status)}</label>)}{!tasks.some(t => t.id !== (editor ? editor.id : '') && t.project_id === form.project_id) && <small>No other task in this project.</small>}</fieldset><label className="wide-field">Files intended for modification<textarea rows={3} placeholder={'One project-relative path per line, e.g.\nsrc/App.tsx'} value={form.intended_files} onChange={e => setForm({ ...form, intended_files: e.target.value })}/><small>Active assignments claim these paths within the project. Parent/child paths overlap. Absolute paths and “..” are rejected.</small></label><label className="wide-field">Files already modified<textarea rows={2} placeholder="One project-relative path per line" value={form.modified_files} onChange={e => setForm({ ...form, modified_files: e.target.value })}/></label><label className="wide-field">Release file claims<textarea rows={2} placeholder="Paths to release from this task, one per line" value={form.released_files} onChange={e => setForm({ ...form, released_files: e.target.value })}/><small>Releasing a path is recorded in task history and removes the claim for that path.</small></label><label className="wide-field">Instructions<textarea rows={3} maxLength={8000} value={form.instructions} onChange={e => setForm({ ...form, instructions: e.target.value })}/></label><label className="wide-field">Completion notes<textarea rows={2} maxLength={8000} value={form.completion_notes} onChange={e => setForm({ ...form, completion_notes: e.target.value })}/></label><label className="wide-field">Blocker<textarea rows={2} maxLength={2000} value={form.blocker} onChange={e => setForm({ ...form, blocker: e.target.value })}/></label></div>
      {error && <div className="inline-error" role="alert">{error}</div>}<div className="modal-actions"><button type="button" className="button-secondary" onClick={() => setEditor(false)}>Cancel</button><button className="button-primary" disabled={busy}>{busy ? 'Saving…' : editor ? 'Save changes' : 'Create task'}</button></div>
    </form></div>}
  </div>
}
