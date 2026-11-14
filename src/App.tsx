import { useCallback, useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type Database from '@tauri-apps/plugin-sql'
import { Activity, AlertCircle, Archive, ArrowDownRight, ArrowUpRight, Bell, CalendarDays, Check, ChevronDown, CircleHelp, Command, FolderKanban, Globe2, LayoutDashboard, ListTodo, MessageSquareText, Plus, Search, Settings, Users, UsersRound, Zap, type LucideIcon } from 'lucide-react'
import { AccountsPage } from './features/accounts/AccountsPage'
import { BrowserProfilesPage } from './features/browsers/BrowserProfilesPage'
import { ProjectsPage } from './features/workforce/ProjectsPage'
import { TasksPage } from './features/workforce/TasksPage'
import { WorkforceCenter } from './features/workforce/WorkforceCenter'
import { RemindersPage } from './features/reminders/RemindersPage'
import type { LaunchFeedback } from './features/browsers/types'
import { errorMessage, getDatabase } from './lib/database'
import './phase2.css'
import './App.css'

const navGroups: { title: string; items: { label: string; icon: LucideIcon; disabled?: boolean }[] }[] = [
  { title: 'OPERATIONS', items: [
    { label: 'Overview', icon: LayoutDashboard }, { label: 'AI Workforce', icon: UsersRound }, { label: 'Accounts', icon: Users }, { label: 'Browser Profiles', icon: Globe2 },
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
interface TaskPreview { id: string; title: string; priority: number; status: string; project_name: string | null; deadline: string | null }

function Overview({ db, onAccounts }: { db: Database; onAccounts: () => void }) {
  const [summary, setSummary] = useState<Summary | null>(null)
  const [quickAccounts, setQuickAccounts] = useState<QuickAccount[]>([])
  const [priorityTasks, setPriorityTasks] = useState<TaskPreview[]>([])
  const [upcomingDeadlines, setUpcomingDeadlines] = useState<TaskPreview[]>([])
  const [launchError, setLaunchError] = useState('')
  const [launchNotice, setLaunchNotice] = useState('')
  const reload = useCallback(async () => {
    const [accountRows, projectRows, taskRows, overdueRows, recentAccounts, priorityRows, deadlineRows] = await Promise.all([
      db.select<{ count: number }[]>("SELECT COUNT(*) AS count FROM accounts WHERE archived_at IS NULL"),
      db.select<{ count: number }[]>("SELECT COUNT(*) AS count FROM projects WHERE archived_at IS NULL AND status='active'"),
      db.select<{ count: number }[]>("SELECT COUNT(*) AS count FROM tasks WHERE archived_at IS NULL AND status!='completed'"),
      db.select<{ count: number }[]>("SELECT COUNT(*) AS count FROM tasks WHERE archived_at IS NULL AND status!='completed' AND deadline IS NOT NULL AND deadline < datetime('now')"),
      db.select<QuickAccount[]>(`SELECT a.id,a.name,bp.profile_name,b.name AS browser_name FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id WHERE a.archived_at IS NULL ORDER BY COALESCE(a.last_accessed_at,'') DESC,a.priority DESC LIMIT 4`),
      db.select<TaskPreview[]>(`SELECT t.id,t.title,t.priority,t.status,p.name AS project_name,t.deadline FROM tasks t LEFT JOIN projects p ON p.id=t.project_id WHERE t.archived_at IS NULL AND t.status!='completed' ORDER BY t.priority DESC,COALESCE(t.deadline,'9999-12-31'),t.created_at LIMIT 4`),
      db.select<TaskPreview[]>(`SELECT t.id,t.title,t.priority,t.status,p.name AS project_name,t.deadline FROM tasks t LEFT JOIN projects p ON p.id=t.project_id WHERE t.archived_at IS NULL AND t.status!='completed' AND t.deadline IS NOT NULL AND t.deadline>=date('now') ORDER BY t.deadline,t.priority DESC LIMIT 4`),
    ])
    setSummary({ accounts: accountRows[0]?.count ?? 0, projects: projectRows[0]?.count ?? 0, tasks: taskRows[0]?.count ?? 0, overdue: overdueRows[0]?.count ?? 0 })
    setQuickAccounts(recentAccounts)
    setPriorityTasks(priorityRows)
    setUpcomingDeadlines(deadlineRows)
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
      <section className="bottom-grid"><article className="panel list-panel overview-list-panel"><div className="panel-heading"><div><h2>Priority tasks</h2><p>Open work ordered by priority and deadline</p></div><button className="text-button" onClick={() => window.dispatchEvent(new CustomEvent('navigate', { detail: 'Tasks' }))}>Open queue <ArrowUpRight size={14}/></button></div>{priorityTasks.length ? priorityTasks.map(task => <button className="overview-task-row" key={task.id} onClick={() => window.dispatchEvent(new CustomEvent('navigate', { detail: 'Tasks' }))}><span className={`work-status ${task.status}`}>{task.status.replaceAll('_', ' ')}</span><strong>{task.title}</strong><small>P{task.priority}{task.project_name ? ` · ${task.project_name}` : ''}</small></button>) : <div className="list-empty"><div className="empty-check"><Check size={16}/></div><div><strong>No open tasks</strong><span>Create tasks in the Task Queue.</span></div></div>}</article><article className="panel list-panel overview-list-panel"><div className="panel-heading"><div><h2>Upcoming deadlines</h2><p>From active task records</p></div></div>{upcomingDeadlines.length ? upcomingDeadlines.map(task => <button className="overview-task-row" key={task.id} onClick={() => window.dispatchEvent(new CustomEvent('navigate', { detail: 'Tasks' }))}><span className="deadline-date">{task.deadline}</span><strong>{task.title}</strong><small>{task.project_name || 'No project'}</small></button>) : <div className="list-empty"><div className="empty-calendar"><CalendarDays size={16}/></div><div><strong>No upcoming deadlines</strong><span>Task deadlines recorded in the database will appear here.</span></div></div>}</article></section>
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
    if (!db) return
    void invoke('start_reminder_scheduler').catch(error => console.error('Reminder scheduler could not start:', errorMessage(error)))
    let stopped = false
    let running = false
    const tick = async () => {
      if (running || stopped) return
      running = true
      try {
        const enabled = await invoke<boolean>('get_auto_assignment')
        if (enabled) {
          const result = await invoke('assign_next_eligible')
          if (result) window.dispatchEvent(new CustomEvent('levi:queue-assignment', { detail: result }))
        }
      } catch (error) {
        window.dispatchEvent(new CustomEvent('levi:queue-error', { detail: errorMessage(error) }))
      } finally { running = false }
    }
    const timer = window.setInterval(() => void tick(), 10000)
    return () => { stopped = true; window.clearInterval(timer) }
  }, [db])
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
      {dbError ? <div className="database-error"><AlertCircle size={18}/><div><strong>Local database could not be opened</strong><span>{dbError}</span></div></div> : !db ? <div className="database-loading">Opening local workspace…</div> : active === 'Overview' ? <Overview db={db} onAccounts={() => setNav('Accounts')}/> : active === 'AI Workforce' ? <WorkforceCenter db={db}/> : active === 'Accounts' ? <AccountsPage db={db}/> : active === 'Browser Profiles' ? <BrowserProfilesPage db={db}/> : active === 'Projects' ? <ProjectsPage db={db}/> : active === 'Tasks' ? <TasksPage db={db}/> : active === 'Reminders' ? <RemindersPage db={db}/> : <div className="future-page"><span className="eyebrow">{active === 'LEVI AI' ? 'INTELLIGENCE' : 'PLANNED CAPABILITY'}</span><h1>{active}</h1><p>This area is reserved for a later roadmap phase. No simulated data or functionality is shown here.</p>{active === 'LEVI AI' && <div className="levi-promise"><MessageSquareText size={19}/><div><strong>LEVI — Command Intelligence</strong><span>Context-aware assistance will be built on verified operations data in a future phase.</span></div></div>}</div>}
    </main>
  </div>
}

export default App
