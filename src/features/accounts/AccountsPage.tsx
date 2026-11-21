import { useCallback, useEffect, useMemo, useState, type FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type Database from '@tauri-apps/plugin-sql'
import { Archive, ArrowDownWideNarrow, ExternalLink, Pencil, Plus, Search, UserRound, X } from 'lucide-react'
import { errorMessage, newId } from '../../lib/database'
import type { AccountRecord, AccountStatus, ProfileOption } from './types'
import type { LaunchFeedback } from '../browsers/types'
import { OpenInSpaceButton } from '../ecosystem/OpenInSpaceButton'
import './accounts.css'

interface ProjectOption { id: string; name: string }
interface FormState {
  name: string; identifier: string; browser_profile_id: string; conversation_url: string
  status: AccountStatus; priority: number; notes: string; project_id: string
}
const emptyForm: FormState = { name: '', identifier: '', browser_profile_id: '', conversation_url: '', status: 'idle', priority: 2, notes: '', project_id: '' }
const statuses: AccountStatus[] = ['working', 'waiting', 'idle', 'blocked', 'completed', 'needs_attention']
const labelStatus = (status: string) => status.replace('_', ' ').replace(/\b\w/g, c => c.toUpperCase())
const dateLabel = (value: string | null) => value ? new Date(value.replace(' ', 'T') + (value.includes('Z') ? '' : 'Z')).toLocaleString() : 'Never opened'

interface Props { db: Database; onChanged?: () => void }
export function AccountsPage({ db, onChanged }: Props) {
  const [accounts, setAccounts] = useState<AccountRecord[]>([])
  const [profiles, setProfiles] = useState<ProfileOption[]>([])
  const [projects, setProjects] = useState<ProjectOption[]>([])
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState('all')
  const [browserFilter, setBrowserFilter] = useState('all')
  const [editor, setEditor] = useState<AccountRecord | null | false>(false)
  const [form, setForm] = useState<FormState>(emptyForm)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')

  const reload = useCallback(async () => {
    const [accountRows, profileRows, projectRows] = await Promise.all([
      db.select<AccountRecord[]>(`SELECT a.id,a.name,a.identifier,a.browser_profile_id,a.conversation_url,a.status,a.priority,a.notes,a.last_accessed_at,
        bp.profile_name,bp.profile_path,b.id AS browser_id,b.name AS browser_name,b.family,
        (SELECT p.id FROM project_accounts pa JOIN projects p ON p.id=pa.project_id WHERE pa.account_id=a.id AND p.archived_at IS NULL ORDER BY pa.assigned_at DESC LIMIT 1) AS project_id,
        (SELECT p.name FROM project_accounts pa JOIN projects p ON p.id=pa.project_id WHERE pa.account_id=a.id AND p.archived_at IS NULL ORDER BY pa.assigned_at DESC LIMIT 1) AS project_name
        FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id
        WHERE a.archived_at IS NULL ORDER BY a.priority DESC,a.name COLLATE NOCASE`),
      db.select<ProfileOption[]>(`SELECT bp.id,bp.browser_id,bp.profile_name,bp.profile_path,bp.availability_status,b.name AS browser_name,b.family
        FROM browser_profiles bp JOIN browsers b ON b.id=bp.browser_id WHERE bp.archived_at IS NULL ORDER BY b.name,bp.profile_name`),
      db.select<ProjectOption[]>("SELECT id,name FROM projects WHERE archived_at IS NULL AND status='active' ORDER BY name COLLATE NOCASE"),
    ])
    setAccounts(accountRows); setProfiles(profileRows); setProjects(projectRows)
    onChanged?.()
  }, [db, onChanged])

  // oxlint-disable-next-line react/set-state-in-effect -- Populate the account view from the local database.
  useEffect(() => { void reload().catch(e => setError(errorMessage(e))) }, [reload])
  const filtered = useMemo(() => accounts.filter(account => {
    const haystack = `${account.name} ${account.identifier ?? ''} ${account.browser_name} ${account.profile_name} ${account.project_name ?? ''}`.toLowerCase()
    return haystack.includes(search.toLowerCase()) && (statusFilter === 'all' || account.status === statusFilter) && (browserFilter === 'all' || account.browser_id === browserFilter)
  }), [accounts, search, statusFilter, browserFilter])
  const openCreate = () => { setEditor(null); setForm({ ...emptyForm, browser_profile_id: profiles.find(p => p.availability_status === 'available')?.id ?? '' }); setError('') }
  const openEdit = (account: AccountRecord) => {
    setEditor(account); setError('')
    setForm({ name: account.name, identifier: account.identifier ?? '', browser_profile_id: account.browser_profile_id, conversation_url: account.conversation_url ?? '', status: account.status, priority: account.priority, notes: account.notes, project_id: account.project_id ?? '' })
  }

  async function save(event: FormEvent) {
    event.preventDefault(); setError('')
    if (!form.name.trim()) { setError('Give this account a name.'); return }
    const profile = profiles.find(p => p.id === form.browser_profile_id)
    if (!profile) { setError('Select a registered browser profile. Add one in Browser Profiles first.'); return }
    if (profile.availability_status !== 'available') { setError('That profile is unavailable. Rescan it or choose an available profile.'); return }
    if (form.conversation_url.trim()) {
      try { const parsed = new URL(form.conversation_url); if (parsed.protocol !== 'https:' || parsed.hostname !== 'claude.ai' || parsed.username || parsed.password) throw new Error() }
      catch { setError('Conversation URLs must be valid https://claude.ai/ links.'); return }
    }
    setBusy(true)
    const id = editor ? editor.id : newId()
    try {
      await invoke('save_account', { account: { id, name: form.name.trim(), identifier: form.identifier.trim() || null, browserProfileId: form.browser_profile_id, conversationUrl: form.conversation_url.trim() || null, status: form.status, priority: form.priority, notes: form.notes.trim(), projectId: form.project_id || null }, isUpdate: Boolean(editor) })
      setEditor(false); setNotice(editor ? 'Account updated.' : 'Account registered.'); await reload()
    } catch (e) {
      setError(errorMessage(e))
    } finally { setBusy(false) }
  }

  async function archive(account: AccountRecord) {
    if (!window.confirm(`Archive “${account.name}”? Its history will be preserved.`)) return
    try {
      await invoke('archive_account', { accountId: account.id })
      setNotice('Account archived.'); await reload()
    } catch (e) { setError(errorMessage(e)) }
  }

  async function launch(account: AccountRecord) {
    setError(''); setNotice(`Launch requested for ${account.name}. Executable/profile validation and a process-conflict check are in progress.`)
    try { const feedback = await invoke<LaunchFeedback>('launch_account', { accountId: account.id }); setNotice(feedback.message); await reload() }
    catch (e) { setError(errorMessage(e)) }
  }

  const browserIds = [...new Map(accounts.map(a => [a.browser_id, a.browser_name])).entries()]
  return <div className="feature-page">
    <div className="feature-header"><div><div className="eyebrow">OPERATIONS / ACCOUNTS</div><h1>Claude accounts</h1><p>Each account opens in its registered browser profile.</p></div><button className="button-primary" onClick={openCreate}><Plus size={16}/> Add account</button></div>
    {notice && <div className="inline-notice">{notice}</div>}{error && !editor && <div className="inline-error" role="alert">{error}</div>}
    <div className="account-toolbar"><label className="searchbox"><Search size={15}/><input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search name, browser, profile, project"/></label><select value={statusFilter} onChange={e => setStatusFilter(e.target.value)}><option value="all">All statuses</option>{statuses.map(status => <option key={status} value={status}>{labelStatus(status)}</option>)}</select><select value={browserFilter} onChange={e => setBrowserFilter(e.target.value)}><option value="all">All browsers</option>{browserIds.map(([id, name]) => <option key={id} value={id}>{name}</option>)}</select><span className="result-count">{filtered.length} {filtered.length === 1 ? 'account' : 'accounts'}</span><button className="sort-label" onClick={() => setAccounts([...accounts].reverse())}><ArrowDownWideNarrow size={14}/> Priority</button></div>
    {filtered.length ? <div className="account-grid">{filtered.map(account => <article className="account-card" key={account.id}>
      <div className="account-card-top"><div className="account-avatar"><UserRound size={18}/></div><div className="account-card-title"><h2>{account.name}</h2><p>{account.identifier || 'No identifier added'}</p></div><span className={`status-pill status-${account.status}`}>{labelStatus(account.status)}</span></div>
      <div className="account-details"><div><span>Browser</span><strong>{account.browser_name}</strong></div><div><span>Profile</span><strong>{account.profile_name}</strong></div><div><span>Project</span><strong>{account.project_name || 'Unassigned'}</strong></div><div><span>Last opened</span><strong>{dateLabel(account.last_accessed_at)}</strong></div></div>
      <div className="account-card-footer"><span className="priority-label">P{account.priority} PRIORITY</span><div><button className="card-icon-button" onClick={() => openEdit(account)} aria-label={`Edit ${account.name}`} title="Edit"><Pencil size={14}/></button><button className="card-icon-button archive-action" onClick={() => void archive(account)} aria-label={`Archive ${account.name}`} title="Archive"><Archive size={14}/></button><OpenInSpaceButton accountId={account.id} accountName={account.name} url={account.conversation_url} onResult={setNotice} /><button className="button-open" onClick={() => void launch(account)}><ExternalLink size={14}/> OPEN</button></div></div>
    </article>)}</div> : <div className="empty-feature"><div className="empty-feature-icon"><UserRound size={20}/></div><strong>{accounts.length ? 'No accounts match these filters' : 'Your account registry is ready'}</strong><p>{accounts.length ? 'Change the search or filters to see more accounts.' : profiles.length ? 'Register an account and assign it to one of your available browser profiles.' : 'Add a browser and profile first, then register your Claude accounts here.'}</p>{!accounts.length && <button className="button-secondary" onClick={() => profiles.length ? openCreate() : window.dispatchEvent(new CustomEvent('navigate', { detail: 'Browser Profiles' }))}><Plus size={14}/>{profiles.length ? 'Add your first account' : 'Add a browser profile'}</button>}</div>}
    {editor !== false && <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) setEditor(false) }}><form className="form-modal" onSubmit={save}>
      <div className="modal-heading"><div><span className="eyebrow">ACCOUNT REGISTRY</span><h2>{editor ? 'Edit account' : 'Register Claude account'}</h2></div><button type="button" className="icon-button" onClick={() => setEditor(false)} aria-label="Close"><X size={17}/></button></div>
      <div className="modal-fields"><label>Account name<input autoFocus required maxLength={80} value={form.name} onChange={e => setForm({ ...form, name: e.target.value })} placeholder="e.g. Claude Research"/></label><label>Identifier <small>Optional; you enter this manually</small><input maxLength={254} value={form.identifier} onChange={e => setForm({ ...form, identifier: e.target.value })} placeholder="Email or account label"/></label>
        <label className="wide-field">Browser profile<select required value={form.browser_profile_id} onChange={e => setForm({ ...form, browser_profile_id: e.target.value })}><option value="">Choose a registered profile</option>{profiles.map(p => <option key={p.id} value={p.id} disabled={p.availability_status !== 'available'}>{p.browser_name} · {p.profile_name}{p.availability_status !== 'available' ? ' (unavailable)' : ''}</option>)}</select></label>
        <label className="wide-field">Claude conversation URL <small>Optional. Host must be claude.ai.</small><input type="url" value={form.conversation_url} onChange={e => setForm({ ...form, conversation_url: e.target.value })} placeholder="https://claude.ai/chat/…"/></label>
        <label>Status<select value={form.status} onChange={e => setForm({ ...form, status: e.target.value as AccountStatus })}>{statuses.map(status => <option key={status} value={status}>{labelStatus(status)}</option>)}</select></label><label>Priority<select value={form.priority} onChange={e => setForm({ ...form, priority: Number(e.target.value) })}><option value={4}>P4 · Urgent</option><option value={3}>P3 · High</option><option value={2}>P2 · Normal</option><option value={1}>P1 · Low</option></select></label>
        <label className="wide-field">Project<select value={form.project_id} onChange={e => setForm({ ...form, project_id: e.target.value })}><option value="">Unassigned</option>{projects.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select></label><label className="wide-field">Notes<textarea rows={3} maxLength={5000} value={form.notes} onChange={e => setForm({ ...form, notes: e.target.value })} placeholder="Instructions or account context"/></label>
      </div>{error && <div className="inline-error" role="alert">{error}</div>}<div className="modal-actions"><button type="button" className="button-secondary" onClick={() => setEditor(false)}>Cancel</button><button className="button-primary" disabled={busy}>{busy ? 'Saving…' : editor ? 'Save changes' : 'Register account'}</button></div>
    </form></div>}
  </div>
}
