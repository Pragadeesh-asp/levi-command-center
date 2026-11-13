import { useCallback, useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type Database from '@tauri-apps/plugin-sql'
import { Activity, AlertCircle, Archive, ArrowDownRight, ArrowUpRight, Bell, CalendarDays, Check, ChevronDown, CircleHelp, Command, FolderKanban, Globe2, LayoutDashboard, ListTodo, MessageSquareText, Plus, Search, Settings, Users, Zap, type LucideIcon } from 'lucide-react'
import { AccountsPage } from './features/accounts/AccountsPage'
import { BrowserProfilesPage } from './features/browsers/BrowserProfilesPage'
import type { LaunchFeedback } from './features/browsers/types'
import { errorMessage, getDatabase } from './lib/database'
import './phase2.css'
import './App.css'

const navGroups: { title: string; items: { label: string; icon: LucideIcon; disabled?: boolean }[] }[] = [
  { title: 'OPERATIONS', items: [
    { label: 'Overview', icon: LayoutDashboard }, { label: 'Accounts', icon: Users }, { label: 'Browser Profiles', icon: Globe2 },
    { label: 'Projects', icon: FolderKanban }, { label: 'Tasks', icon: ListTodo }, { label: 'Output Vault', icon: Archive },
  ] },
  { title: 'INTELLIGENCE', items: [{ label: 'LEVI AI', icon: MessageSquareText, disabled: true }] },
  { title: 'CONTROL', items: [
    { label: 'Calendar', icon: CalendarDays }, { label: 'Reminders', icon: Bell }, { label: 'Activity', icon: Activity },
    { label: 'Analytics', icon: Activity }, { label: 'Settings', icon: Settings },
  ] },
]
interface Summary { accounts: number; projects: number; tasks: number; overdue: number }
interface QuickAccount { id: string; name: string; profile_name: string; browser_name: string }

function Overview({ db, onAccounts }: { db: Database; onAccounts: () => void }) {
  const [summary, setSummary] = useState<Summary | null>(null)
  const [quickAccounts, setQuickAccounts] = useState<QuickAccount[]>([])
  const [launchError, setLaunchError] = useState('')
  const [launchNotice, setLaunchNotice] = useState('')
  const reload = useCallback(async () => {
    const [accountRows, projectRows, taskRows, overdueRows, recentAccounts] = await Promise.all([
      db.select<{ count: number }[]>("SELECT COUNT(*) AS count FROM accounts WHERE archived_at IS NULL"),
      db.select<{ count: number }[]>("SELECT COUNT(*) AS count FROM projects WHERE archived_at IS NULL AND status='active'"),
      db.select<{ count: number }[]>("SELECT COUNT(*) AS count FROM tasks WHERE archived_at IS NULL AND status!='completed'"),
      db.select<{ count: number }[]>("SELECT COUNT(*) AS count FROM tasks WHERE archived_at IS NULL AND status!='completed' AND deadline IS NOT NULL AND deadline < datetime('now')"),
      db.select<QuickAccount[]>(`SELECT a.id,a.name,bp.profile_name,b.name AS browser_name FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id WHERE a.archived_at IS NULL ORDER BY COALESCE(a.last_accessed_at,'') DESC,a.priority DESC LIMIT 4`),
    ])
    setSummary({ accounts: accountRows[0]?.count ?? 0, projects: projectRows[0]?.count ?? 0, tasks: taskRows[0]?.count ?? 0, overdue: overdueRows[0]?.count ?? 0 })
    setQuickAccounts(recentAccounts)
  }, [db])
  // oxlint-disable-next-line react/set-state-in-effect -- Load real SQLite summary data when this view is mounted.
  useEffect(() => { void reload().catch(e => setLaunchError(errorMessage(e))) }, [reload])
  const cards = [
    { label: 'Claude accounts', value: summary?.accounts, detail: 'Registered and available to manage', icon: Users, tone: 'blue' },
    { label: 'Active projects', value: summary?.projects, detail: 'Projects currently in progress', icon: FolderKanban, tone: 'cyan' },
    { label: 'Open tasks', value: summary?.tasks, detail: 'Tasks not yet completed', icon: ListTodo, tone: 'violet' },
    { label: 'Overdue', value: summary?.overdue, detail: 'Open tasks past their deadline', icon: AlertCircle, tone: 'amber' },
  ]
  async function launch(id: string, name: string) {
    setLaunchError(''); setLaunchNotice(`Launch requested for ${name}. Executable/profile validation and a process-conflict check are in progress.`)
    try { const feedback = await invoke<LaunchFeedback>('launch_account', { accountId: id }); setLaunchNotice(feedback.message); await reload() }
    catch (e) { setLaunchError(errorMessage(e)) }
  }
  return <>
    <div className="content"><div className="page-heading"><div><div className="eyebrow"><span className="live-pulse"/> YOUR OPERATIONS, AT A GLANCE</div><h1>Good afternoon<span className="heading-period">.</span></h1><p className="subtitle">Your Personal AI Operations Headquarters.</p></div><div className="heading-actions"><button className="button-secondary"><CalendarDays size={15}/> Today <ChevronDown size={14}/></button><button className="button-primary" onClick={onAccounts}><Plus size={16}/> Add account</button></div></div>
      {launchNotice && <div className="inline-notice">{launchNotice}</div>}{launchError && <div className="inline-error" role="alert">{launchError}</div>}
      <section className="metrics-grid">{cards.map(({label,value,detail,icon:Icon,tone})=><article className="metric-card" key={label}><div className="metric-top"><span>{label}</span><div className={`metric-icon ${tone}`}><Icon size={16}/></div></div><div className="metric-value">{value ?? '—'}</div><div className="metric-detail">{detail}</div><div className={`metric-trend ${tone}`}><ArrowDownRight size={13}/>{summary ? 'From local records' : 'Loading records…'}</div></article>)}</section>
      <section className="overview-grid"><article className="panel activity-panel"><div className="panel-heading"><div><h2>Account activity</h2><p>Recorded account events from your workspace</p></div><button className="text-button" onClick={() => window.dispatchEvent(new CustomEvent('navigate', { detail: 'Activity' }))}>View activity <ArrowUpRight size={14}/></button></div><div className="chart-placeholder"><div className="chart-y"><span>10</span><span>8</span><span>6</span><span>4</span><span>2</span><span>0</span></div><div className="chart-area"><div className="chart-empty"><div className="chart-orb"><Activity size={20}/></div><strong>Activity timeline is coming in a later phase</strong><span>Browser presence alone does not indicate that a Claude account is working.</span></div><div className="chart-x"><span>MON</span><span>TUE</span><span>WED</span><span>THU</span><span>FRI</span><span>SAT</span><span>SUN</span></div></div></div><div className="chart-legend"><span><i className="legend-dot cyan-dot"/> Working</span><span><i className="legend-dot blue-dot"/> Waiting</span><span><i className="legend-dot grey-dot"/> Idle</span><span className="chart-period">LOCAL HISTORY</span></div></article>
        <article className="panel quick-panel"><div className="panel-heading"><div><h2>Quick launch</h2><p>Open an account in its assigned profile</p></div><Zap size={16} className="quick-zap"/></div>{quickAccounts.length ? <div className="quick-account-list">{quickAccounts.map(account => <div className="quick-account-row" key={account.id}><div><strong>{account.name}</strong><span>{account.browser_name} · {account.profile_name}</span></div><button className="button-open" onClick={() => void launch(account.id, account.name)}>OPEN</button></div>)}</div> : <div className="quick-empty"><div className="quick-icon"><Users size={19}/></div><strong>No accounts registered</strong><span>Add browser profiles, then assign accounts to them.</span><button className="quick-add" onClick={onAccounts}><Plus size={14}/> Register account</button></div>}<div className="quick-foot"><span>ACCOUNT REGISTRY</span><span>{quickAccounts.length ? `${quickAccounts.length} RECENT` : 'READY WHEN YOU ARE'}</span></div></article></section>
      <section className="bottom-grid"><article className="panel list-panel"><div className="panel-heading"><div><h2>Priority tasks</h2><p>Tasks will appear when task management is implemented</p></div></div><div className="list-empty"><div className="empty-check"><Check size={16}/></div><div><strong>Task management is planned</strong><span>No task data is being estimated before that phase.</span></div></div></article><article className="panel list-panel"><div className="panel-heading"><div><h2>Upcoming deadlines</h2><p>Project and task planning is a later phase</p></div></div><div className="list-empty"><div className="empty-calendar"><CalendarDays size={16}/></div><div><strong>No deadline records yet</strong><span>Deadlines will show here once the related modules are built.</span></div></div></article></section>
      <footer><span><span className="online-dot"/> All data stored locally on this device</span><span>LEVI COMMAND CENTER <i>•</i> BUILT FOR CLARITY <ArrowUpRight size={11}/></span></footer>
    </div>
  </>
}

function App() {
  const [active, setActive] = useState('Overview')
  const [query, setQuery] = useState('')
  const [db, setDb] = useState<Database | null>(null)
  const [dbError, setDbError] = useState('')
  // oxlint-disable-next-line react/set-state-in-effect -- Connect to the persisted SQLite workspace at startup.
  useEffect(() => { void getDatabase().then(setDb).catch(e => setDbError(errorMessage(e))) }, [])
  useEffect(() => {
    const handler = (event: Event) => setActive((event as CustomEvent<string>).detail)
    window.addEventListener('navigate', handler)
    return () => window.removeEventListener('navigate', handler)
  }, [])
  const setNav = (label: string) => setActive(label)
  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><div className="brand-mark"><Command size={18}/></div><div><strong>LEVI COMMAND CENTER</strong><span>AI OPERATIONS HEADQUARTERS</span></div></div>
      <div className="workspace-label">WORKSPACE <ChevronDown size={13}/></div>
      <div className="workspace"><div className="workspace-avatar">L</div><div><strong>Personal workspace</strong><small>Local environment</small></div><span className="local-dot"/></div>
      {navGroups.map(group => <div className="nav-group" key={group.title}><div className="nav-caption">{group.title}</div><nav>{group.items.map(({label, icon: Icon, disabled}) => <button key={label} disabled={disabled} title={disabled ? 'LEVI AI is planned for a later phase.' : undefined} className={`nav-item ${active === label ? 'selected' : ''} ${disabled ? 'nav-disabled' : ''}`} onClick={() => !disabled && setNav(label)}><Icon size={17}/><span>{label}</span>{disabled && <small>PLANNED</small>}</button>)}</nav></div>)}
      <div className="sidebar-bottom"><div className="help-card"><div className="help-icon"><CircleHelp size={16}/></div><strong>Need a hand?</strong><p>Get started with your command center.</p><button onClick={() => setNav('Settings')}>View setup guide <ArrowUpRight size={13}/></button></div><div className="build-label"><span className="online-dot"/> LOCAL SYSTEM <span>v0.2.0</span></div></div>
    </aside>
    <main className="main-area">
      <header className="topbar"><div className="breadcrumb">LEVI COMMAND CENTER <span>/</span> <strong>{active}</strong></div><div className="top-actions"><label className="searchbox"><Search size={15}/><input value={query} onChange={e => setQuery(e.target.value)} placeholder="Search accounts and records..."/><kbd>⌘ K</kbd></label><button className="icon-button" aria-label="Notifications"><Bell size={17}/><i/></button><div className="avatar">L</div></div></header>
      {dbError ? <div className="database-error"><AlertCircle size={18}/><div><strong>Local database could not be opened</strong><span>{dbError}</span></div></div> : !db ? <div className="database-loading">Opening local workspace…</div> : active === 'Overview' ? <Overview db={db} onAccounts={() => setNav('Accounts')}/> : active === 'Accounts' ? <AccountsPage db={db}/> : active === 'Browser Profiles' ? <BrowserProfilesPage db={db}/> : <div className="future-page"><span className="eyebrow">{active === 'LEVI AI' ? 'INTELLIGENCE' : 'PLANNED CAPABILITY'}</span><h1>{active}</h1><p>This area is reserved for a later roadmap phase. No simulated data or functionality is shown here.</p>{active === 'LEVI AI' && <div className="levi-promise"><MessageSquareText size={19}/><div><strong>LEVI — Command Intelligence</strong><span>Context-aware assistance will be built on verified operations data in a future phase.</span></div></div>}</div>}
    </main>
  </div>
}

export default App
