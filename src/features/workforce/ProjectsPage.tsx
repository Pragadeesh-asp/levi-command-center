import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type Database from '@tauri-apps/plugin-sql'
import { Archive, Pencil, Plus, X } from 'lucide-react'
import { errorMessage, newId } from '../../lib/database'
import { MagicBentoCard } from './MagicBentoCard'
import './workforce.css'

interface Project { id: string; name: string; description: string; status: string; priority: number; start_date: string | null; deadline: string | null; notes: string; task_count: number; completed_task_count: number; account_count: number; account_ids: string[] }
interface Account { id: string; name: string; status: string }
interface Form { name: string; description: string; status: string; priority: number; start_date: string; deadline: string; notes: string; account_ids: string[] }
const empty: Form = { name: '', description: '', status: 'active', priority: 2, start_date: '', deadline: '', notes: '', account_ids: [] }
const priorities = [{ value: 4, label: 'P4 · Urgent' }, { value: 3, label: 'P3 · High' }, { value: 2, label: 'P2 · Normal' }, { value: 1, label: 'P1 · Low' }]

export function ProjectsPage({ db }: { db: Database }) {
  const [projects, setProjects] = useState<Project[]>([])
  const [accounts, setAccounts] = useState<Account[]>([])
  const [editor, setEditor] = useState<Project | null | false>(false)
  const [form, setForm] = useState<Form>(empty)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)
  const reload = useCallback(async () => {
    const [projectRows, membershipRows, accountRows] = await Promise.all([
    db.select<Omit<Project, 'account_ids'>[]>(`SELECT p.id,p.name,p.description,p.status,p.priority,p.start_date,p.deadline,p.notes,
    (SELECT COUNT(*) FROM tasks t WHERE t.project_id=p.id AND t.archived_at IS NULL) AS task_count,
    (SELECT COUNT(*) FROM tasks t WHERE t.project_id=p.id AND t.archived_at IS NULL AND t.status='completed') AS completed_task_count,
    (SELECT COUNT(*) FROM project_accounts pa JOIN accounts a ON a.id=pa.account_id WHERE pa.project_id=p.id AND a.archived_at IS NULL) AS account_count
    FROM projects p WHERE p.archived_at IS NULL ORDER BY CASE p.status WHEN 'active' THEN 0 WHEN 'paused' THEN 1 ELSE 2 END,p.priority DESC,p.name COLLATE NOCASE`),
      db.select<{project_id:string;account_id:string}[]>('SELECT project_id,account_id FROM project_accounts'),
      db.select<Account[]>('SELECT id,name,status FROM accounts WHERE archived_at IS NULL ORDER BY name COLLATE NOCASE'),
    ])
    setAccounts(accountRows)
    setProjects(projectRows.map(project => ({ ...project, account_ids: membershipRows.filter(row => row.project_id === project.id).map(row => row.account_id) })))
  }, [db])
  // oxlint-disable-next-line react/set-state-in-effect -- Load persisted project records when this page mounts.
  useEffect(() => { void reload().catch(e => setError(errorMessage(e))) }, [reload])
  function create() { setEditor(null); setForm(empty); setError('') }
  function edit(project: Project) { setEditor(project); setForm({ name: project.name, description: project.description, status: project.status, priority: project.priority, start_date: project.start_date ?? '', deadline: project.deadline ?? '', notes: project.notes, account_ids: project.account_ids }); setError('') }
  async function save(event: FormEvent) {
    event.preventDefault(); setError(''); setBusy(true)
    try {
      await invoke('save_project', { project: { id: editor ? editor.id : newId(), name: form.name, description: form.description, status: form.status, priority: form.priority, startDate: form.start_date || null, deadline: form.deadline || null, notes: form.notes, accountIds: form.account_ids }, isUpdate: Boolean(editor) })
      setEditor(false); setNotice(editor ? 'Project updated.' : 'Project created.'); await reload()
    } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }
  async function archive(project: Project) {
    if (!window.confirm(`Archive “${project.name}”? Its tasks and history will be preserved.`)) return
    setError(''); setNotice('')
    try { await invoke('archive_project', { projectId: project.id }); setNotice('Project archived.'); await reload() }
    catch (e) { setError(errorMessage(e)) }
  }
  return <div className="feature-page workforce-page">
    <div className="feature-header"><div><div className="eyebrow">OPERATIONS / PROJECTS</div><h1>Projects</h1><p>Organize work and registered accounts in local project records.</p></div><button className="button-primary" onClick={create}><Plus size={16}/> New project</button></div>
    {notice && <div className="inline-notice" role="status">{notice}</div>}{error && !editor && <div className="inline-error" role="alert">{error}</div>}
    {projects.length ? <div className="project-grid magic-bento-grid">{projects.map(project => <MagicBentoCard className="work-card" key={project.id}>
      <div className="work-card-top"><div><span className={`work-status ${project.status}`}>{project.status.replace('_', ' ')}</span><h2>{project.name}</h2></div><span className="priority-label">P{project.priority}</span></div>
      <p className="work-description">{project.description || 'No project description.'}</p>
      <div className="project-progress" aria-label={`Project progress ${project.completed_task_count} of ${project.task_count} tasks complete`}><span><i style={{width:`${project.task_count?Math.round(project.completed_task_count/project.task_count*100):0}%`}}/></span><small>{project.task_count?Math.round(project.completed_task_count/project.task_count*100):0}% · {project.completed_task_count}/{project.task_count} complete</small></div>
      <div className="work-stats"><span>{project.task_count} tasks</span><span>{project.account_count} assigned accounts</span>{project.deadline && <span>Due {project.deadline}</span>}</div>
      <div className="work-card-actions"><button className="button-secondary" onClick={() => edit(project)}><Pencil size={13}/> Edit</button><button className="card-icon-button archive-action" onClick={() => void archive(project)} aria-label={`Archive ${project.name}`} title="Archive project"><Archive size={14}/></button></div>
    </MagicBentoCard>)}</div> : <div className="empty-feature"><strong>No projects yet</strong><p>Create a project before adding project-specific tasks or mapping accounts.</p><button className="button-primary" onClick={create}><Plus size={14}/> Create your first project</button></div>}
    {editor !== false && <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) setEditor(false) }}><form className="form-modal" onSubmit={save}>
      <div className="modal-heading"><div><span className="eyebrow">PROJECT RECORD</span><h2>{editor ? 'Edit project' : 'Create project'}</h2></div><button type="button" className="icon-button" onClick={() => setEditor(false)} aria-label="Close"><X size={17}/></button></div>
      <div className="modal-fields"><label className="wide-field">Project name<input autoFocus required maxLength={120} value={form.name} onChange={e => setForm({ ...form, name: e.target.value })}/></label><label className="wide-field">Description<textarea rows={3} maxLength={4000} value={form.description} onChange={e => setForm({ ...form, description: e.target.value })}/></label><label>Status<select value={form.status} onChange={e => setForm({ ...form, status: e.target.value })}><option value="active">Active</option><option value="paused">Paused</option><option value="completed">Completed</option></select></label><label>Priority<select value={form.priority} onChange={e => setForm({ ...form, priority: Number(e.target.value) })}>{priorities.map(p => <option key={p.value} value={p.value}>{p.label}</option>)}</select></label><label>Start date<input type="date" value={form.start_date} onChange={e => setForm({ ...form, start_date: e.target.value })}/></label><label>Deadline<input type="date" value={form.deadline} onChange={e => setForm({ ...form, deadline: e.target.value })}/></label><label className="wide-field">Notes<textarea rows={3} maxLength={8000} value={form.notes} onChange={e => setForm({ ...form, notes: e.target.value })}/></label><fieldset className="wide-field dependency-picker"><legend>Project accounts</legend>{accounts.map(account => <label key={account.id}><input type="checkbox" checked={form.account_ids.includes(account.id)} onChange={e => setForm({ ...form, account_ids: e.target.checked ? [...form.account_ids, account.id] : form.account_ids.filter(id => id !== account.id) })}/>{account.name} · {account.status}</label>)}{!accounts.length && <small>Register an account before assigning workers to this project.</small>}<small>Project membership controls which accounts may receive this project’s tasks. Availability is checked when a task is assigned.</small></fieldset></div>
      {error && <div className="inline-error" role="alert">{error}</div>}<div className="modal-actions"><button type="button" className="button-secondary" onClick={() => setEditor(false)}>Cancel</button><button className="button-primary" disabled={busy}>{busy ? 'Saving…' : editor ? 'Save changes' : 'Create project'}</button></div>
    </form></div>}
  </div>
}
