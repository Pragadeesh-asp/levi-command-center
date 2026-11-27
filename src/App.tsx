import { Suspense, lazy, useCallback, useEffect, useRef, useState, type FormEvent, type PointerEvent as ReactPointerEvent, type WheelEvent as ReactWheelEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type Database from '@tauri-apps/plugin-sql'
import { Activity, AlertCircle, Archive, ArrowDownRight, ArrowUpRight, Bell, Bot, CalendarDays, Check, ChevronDown, ChevronLeft, Command, FolderKanban, Globe2, History, LayoutDashboard, ListTodo, Menu, MessageSquareText, Plus, Search, Settings, Square, Users, UsersRound, Volume2, Workflow, X, Zap, type LucideIcon } from 'lucide-react'
import { AccountsPage } from './features/accounts/AccountsPage'
import { BrowserProfilesPage } from './features/browsers/BrowserProfilesPage'
import { ProjectsPage } from './features/workforce/ProjectsPage'
import { TasksPage } from './features/workforce/TasksPage'
import { WorkforceCenter } from './features/workforce/WorkforceCenter'
import { RemindersPage } from './features/reminders/RemindersPage'
import { CheckpointCenter } from './features/workforce/CheckpointCenter'
import { HandoffCenter } from './features/workforce/HandoffCenter'
import { IntelligenceCenter } from './features/intelligence/IntelligenceCenter'
import { ThoughtLine, type ThoughtLineStatus } from './features/intelligence/ThoughtLine'
import { ComposerActionMenu } from './features/intelligence/ComposerActionMenu'
import { DashboardSearch } from './features/workforce/DashboardSearch'
import { useReducedMotion } from './lib/useReducedMotion'
import { PromptBar } from './features/intelligence/PromptBar'
import { AppSwitcher } from './features/ecosystem/AppSwitcher'
import { usageLabel, type MessageUsage } from './features/intelligence/usage'
import type { LaunchFeedback } from './features/browsers/types'
import { errorMessage, getDatabase } from './lib/database'
import { BottomSheet, type SheetSize } from './features/shell/BottomSheet'
import { ConfirmHost } from './features/shell/ConfirmHost'
import './phase2.css'
import './App.css'
import './levi-theme.css'
import './global-assistant.css'
import './effects.css'
import './desktop-overhaul.css'
import './levi-carbon.css'
import './levi-nav.css'

const MarkdownMessage = lazy(() => import('./features/intelligence/MarkdownMessage'))
import { SettingsPage } from './features/settings/SettingsPage'
type AssistantSlot = 'primary' | 'secondary'
interface AssistantConfig { slot: AssistantSlot; provider: string; baseUrl: string; model: string; hasApiKey: boolean }
interface AssistantConversation { id: string; title: string; updatedAt: string }
interface AssistantMessage extends MessageUsage { id: string; role: string; responseKind: string; requestStatus: string | null; content: string; provider: string | null; model: string | null; createdAt: string }
interface AssistantProposal { assistantMessageId: string; status: string }

const navGroups: { title: string; icon: LucideIcon; items: { label: string; icon: LucideIcon; disabled?: boolean }[] }[] = [
  { title: 'WORK', icon: LayoutDashboard, items: [
    { label: 'Overview', icon: LayoutDashboard }, { label: 'Projects', icon: FolderKanban }, { label: 'Tasks', icon: ListTodo },
  ] },
  { title: 'AI', icon: Bot, items: [
    { label: 'AI Command', icon: MessageSquareText }, { label: 'AI Workforce', icon: UsersRound }, { label: 'Checkpoint Center', icon: History },
    { label: 'Handoff Center', icon: Workflow }, { label: 'Output Vault', icon: Archive },
  ] },
  { title: 'ACCOUNTS', icon: Users, items: [{ label: 'Accounts', icon: Users }, { label: 'Browser Profiles', icon: Globe2 }] },
  { title: 'TIME', icon: CalendarDays, items: [{ label: 'Calendar', icon: CalendarDays }, { label: 'Reminders', icon: Bell }] },
  { title: 'INSIGHT', icon: Activity, items: [
    { label: 'Activity', icon: Activity }, { label: 'Analytics', icon: Activity }, { label: 'Settings', icon: Settings },
  ] },
]
interface Summary { accounts: number; projects: number; tasks: number; overdue: number }
interface QuickAccount { id: string; name: string; profile_name: string; browser_name: string }
interface TaskPreview { id: string; title: string; priority: number; status: string; project_name: string | null; deadline: string | null }

function Overview({ db, onAccounts, query, onQuery, onNavigate }: { db: Database; onAccounts: () => void; query: string; onQuery: (value: string) => void; onNavigate: (section: string) => void }) {
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
    { label: 'Claude accounts', value: summary?.accounts, detail: 'Registered and available to manage', icon: Users, tone: 'blue', target: 'Accounts' },
    { label: 'Active projects', value: summary?.projects, detail: 'Projects currently in progress', icon: FolderKanban, tone: 'cyan', target: 'Projects' },
    { label: 'Open tasks', value: summary?.tasks, detail: 'Tasks not yet completed', icon: ListTodo, tone: 'violet', target: 'Tasks' },
    { label: 'Overdue', value: summary?.overdue, detail: 'Open tasks past their deadline', icon: AlertCircle, tone: 'amber', target: 'Tasks' },
  ]
  async function launch(id: string, name: string) {
    setLaunchError(''); setLaunchNotice(`Launch requested for ${name}. Executable/profile validation and a process-conflict check are in progress.`)
    try { const feedback = await invoke<LaunchFeedback>('launch_account', { accountId: id }); setLaunchNotice(feedback.message); await reload() }
    catch (e) { setLaunchError(errorMessage(e)) }
  }
  return <>
    <div className="content"><div className="page-heading"><div><div className="eyebrow"><span className="live-pulse"/> YOUR OPERATIONS, AT A GLANCE</div><h1>Good afternoon<span className="heading-period">.</span></h1><p className="subtitle">Your Personal AI Operations Headquarters.</p></div><div className="heading-actions"><button className="button-secondary"><CalendarDays size={15}/> Today <ChevronDown size={14}/></button><button className="button-primary" onClick={onAccounts}><Plus size={16}/> Add account</button></div></div>
      <DashboardSearch db={db} query={query} onQuery={onQuery} onNavigate={onNavigate}/>
      {launchNotice && <div className="inline-notice">{launchNotice}</div>}{launchError && <div className="inline-error" role="alert">{launchError}</div>}
      <section className="metrics-grid">{cards.map(({label,value,detail,icon:Icon,tone,target})=><button type="button" className="metric-card levi-spoke" key={label} onClick={() => onNavigate(target)} title={`Open ${target}`}><div className="metric-top"><span>{label}</span><div className={`metric-icon ${tone}`}><Icon size={16}/></div></div><div className="metric-value">{value ?? '—'}</div><div className="metric-detail">{detail}</div><div className={`metric-trend ${tone}`}><ArrowDownRight size={13}/>{summary ? `Open ${target}` : 'Loading records…'}<ArrowUpRight size={13}/></div></button>)}</section>
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
  const [assistantOpen, setAssistantOpen] = useState(false)
  const [sidebarOpen, setSidebarOpen] = useState(false)
  const [history, setHistory] = useState<string[]>([])
  const [assistantSize, setAssistantSize] = useState<SheetSize>('half')
  const activeRef = useRef('Overview')
  const swipe = useRef<{ x: number; y: number; edge: boolean } | null>(null)
  const wheel = useRef({ dx: 0, until: 0 })
  const [voiceName, setVoiceName] = useState(() => localStorage.getItem('levi.voice.name') ?? '')
  const [speechRate, setSpeechRate] = useState(() => Number(localStorage.getItem('levi.voice.rate') ?? '1'))
  useEffect(() => { localStorage.setItem('levi.voice.name', voiceName) }, [voiceName])
  useEffect(() => { localStorage.setItem('levi.voice.rate', String(speechRate)) }, [speechRate])
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
    const handler = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        window.dispatchEvent(new CustomEvent('navigate', { detail: 'Overview' }))
        requestAnimationFrame(() => document.getElementById('global-record-search')?.focus())
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.altKey && event.shiftKey && event.key.toLowerCase() === 'l' && !event.repeat) {
        event.preventDefault()
        setAssistantOpen(value => !value)
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])
  useEffect(() => {
    const handler = (event: KeyboardEvent) => { if (event.key === 'Escape') { setSidebarOpen(false); setAssistantOpen(false) } }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])
  /* Navigation: every move to another section is recorded, so Back (Alt+Left, the mouse
     back button or a swipe from the left edge) returns along the same path. */
  const goTo = (label: string) => {
    setSidebarOpen(false)
    const previous = activeRef.current
    if (previous === label) return
    activeRef.current = label
    setHistory(stack => [...stack.slice(-29), previous])
    setActive(label)
  }
  const back = () => {
    setSidebarOpen(false)
    if (!history.length) return
    const previous = history[history.length - 1]
    activeRef.current = previous
    setHistory(history.slice(0, -1))
    setActive(previous)
  }
  const groupIndex = Math.max(0, navGroups.findIndex(navGroup => navGroup.items.some(item => item.label === active)))
  const group = navGroups[groupIndex]
  const tabIndex = group.items.findIndex(item => item.label === active)
  const moveTab = (step: number) => {
    const next = group.items[tabIndex + step]
    if (next && !next.disabled) goTo(next.label)
  }
  // Re-registered on each render so the shortcuts always see the current section and history.
  useEffect(() => {
    const onNavigate = (event: Event) => goTo((event as CustomEvent<string>).detail)
    const onKey = (event: KeyboardEvent) => {
      if (event.altKey && !event.ctrlKey && !event.shiftKey && /^[1-5]$/.test(event.key)) { event.preventDefault(); goTo(navGroups[Number(event.key) - 1].items[0].label); return }
      if (event.altKey && event.key === 'ArrowLeft') { event.preventDefault(); back(); return }
      if (event.ctrlKey && event.key === 'Tab') { event.preventDefault(); moveTab(event.shiftKey ? -1 : 1); return }
      if (event.ctrlKey && !event.shiftKey && event.key.toLowerCase() === 'm') { event.preventDefault(); setSidebarOpen(value => !value) }
    }
    const onMouse = (event: MouseEvent) => { if (event.button === 3) { event.preventDefault(); back() } }
    window.addEventListener('navigate', onNavigate)
    window.addEventListener('keydown', onKey)
    window.addEventListener('mouseup', onMouse)
    return () => { window.removeEventListener('navigate', onNavigate); window.removeEventListener('keydown', onKey); window.removeEventListener('mouseup', onMouse) }
  })

  /* Gestures. Touch or pen: swipe sideways to change tab, swipe in from the left edge to go back.
     Touchpad: a two-finger sideways swipe changes tab, unless the pointer is over something that scrolls sideways. */
  function scrollsSideways(target: EventTarget | null) {
    for (let node = target instanceof Element ? target : null; node && node !== document.body; node = node.parentElement) {
      if (node.matches('input, textarea, select, [contenteditable="true"]')) return true
      if (node.scrollWidth > node.clientWidth + 2 && /(auto|scroll)/.test(getComputedStyle(node).overflowX)) return true
    }
    return false
  }
  function pointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.pointerType === 'mouse') return
    swipe.current = { x: event.clientX, y: event.clientY, edge: event.clientX - event.currentTarget.getBoundingClientRect().left < 28 }
  }
  function pointerUp(event: ReactPointerEvent<HTMLDivElement>) {
    const start = swipe.current
    swipe.current = null
    if (!start) return
    const dx = event.clientX - start.x
    const dy = event.clientY - start.y
    if (Math.abs(dx) < 80 || Math.abs(dy) > Math.abs(dx) * 0.6) return
    if (start.edge && dx > 0) { if (history.length) back(); else setSidebarOpen(true); return }
    if (!scrollsSideways(event.target)) moveTab(dx < 0 ? 1 : -1)
  }
  function wheelSwipe(event: ReactWheelEvent<HTMLDivElement>) {
    if (Math.abs(event.deltaX) < Math.abs(event.deltaY) * 1.5 || scrollsSideways(event.target)) return
    const now = performance.now()
    if (now < wheel.current.until) return
    wheel.current.dx += event.deltaX
    if (Math.abs(wheel.current.dx) > 160) { moveTab(wheel.current.dx > 0 ? 1 : -1); wheel.current = { dx: 0, until: now + 600 } }
  }

  const GroupIcon = group.icon
  return <div className={`app-shell levi-nav-shell ${sidebarOpen ? 'sidebar-open' : ''}`}>
    {sidebarOpen && <button type="button" className="sidebar-scrim" aria-label="Close navigation" onClick={() => setSidebarOpen(false)}/>}
    <aside className="sidebar levi-drawer" aria-label="All sections" aria-hidden={!sidebarOpen} inert={!sidebarOpen}>
      <div className="levi-drawer-head"><div className="brand"><div className="brand-mark"><Command size={18}/></div><div><strong>LEVI COMMAND CENTER</strong><span>ALL SECTIONS · CTRL+M</span></div></div><button type="button" className="levi-drawer-close" onClick={() => setSidebarOpen(false)} aria-label="Close all sections"><X size={18}/></button></div>
      <div className="levi-drawer-grid">
        {navGroups.map((navGroup, index) => <div className="nav-group" key={navGroup.title}><div className="nav-caption">{navGroup.title}<kbd>Alt+{index + 1}</kbd></div><nav>{navGroup.items.map(({label, icon: Icon, disabled}) => <button key={label} disabled={disabled} className={`nav-item ${active === label ? 'selected' : ''} ${disabled ? 'nav-disabled' : ''}`} onClick={() => !disabled && goTo(label)}><Icon size={16}/><span>{label}</span></button>)}</nav></div>)}
      </div>
      <div className="build-label"><span className="online-dot"/> LOCAL SYSTEM <span>v0.2.0</span></div>
    </aside>
    <main className="main-area">
      <header className="topbar">
        <button type="button" className="navigation-toggle" aria-label={sidebarOpen ? 'Close all sections' : 'Open all sections'} title="All sections (Ctrl+M)" aria-expanded={sidebarOpen} onClick={() => setSidebarOpen(value => !value)}>{sidebarOpen ? <X size={19}/> : <Menu size={19}/>}</button>
        {history.length > 0 && <button type="button" className="levi-back" onClick={back} aria-label={`Back to ${history[history.length - 1]}`} title={`Back to ${history[history.length - 1]} (Alt+Left)`}><ChevronLeft size={18}/></button>}
        <nav className="breadcrumb levi-crumbs" aria-label="You are here"><button type="button" onClick={() => goTo(group.items[0].label)}><GroupIcon size={14}/>{group.title}</button><span>/</span><strong>{active}</strong></nav>
        <div className="top-actions"><label className="searchbox"><Search size={15}/><input id="global-record-search" value={query} onFocus={() => { if (active !== 'Overview') goTo('Overview') }} onChange={e => setQuery(e.target.value)} placeholder="Search anything in LEVI…"/><kbd>Ctrl K</kbd></label><AppSwitcher /><button type="button" className={`levi-jarvis-button ${assistantOpen ? 'is-open' : ''}`} onClick={() => setAssistantOpen(value => !value)} aria-expanded={assistantOpen} aria-controls="global-assistant-panel" title="JARVIS (Alt+Shift+L)"><Bot size={16}/><span>JARVIS</span><kbd>Alt⇧L</kbd></button><div className="avatar">L</div></div>
      </header>
      {group.items.length > 1 && <div className="levi-toptabs" role="tablist" aria-label={`${group.title} sections`}>{group.items.map(({label, icon: Icon, disabled}) => <button key={label} type="button" role="tab" aria-selected={active === label} disabled={disabled} className={active === label ? 'is-on' : ''} onClick={() => goTo(label)}><Icon size={14}/>{label}</button>)}</div>}
      <div className="workspace-scroll" onPointerDown={pointerDown} onPointerUp={pointerUp} onPointerCancel={() => { swipe.current = null }} onWheel={wheelSwipe}>
      {dbError ? <div className="database-error"><AlertCircle size={18}/><div><strong>Local database could not be opened</strong><span>{dbError}</span></div></div> : !db ? <div className="database-loading">Opening local workspace…</div> : active === 'Overview' ? <Overview db={db} onAccounts={() => goTo('Accounts')} query={query} onQuery={setQuery} onNavigate={goTo}/> : active === 'AI Workforce' ? <WorkforceCenter db={db}/> : active === 'Accounts' ? <AccountsPage db={db}/> : active === 'Browser Profiles' ? <BrowserProfilesPage db={db}/> : active === 'Projects' ? <ProjectsPage db={db}/> : active === 'Tasks' ? <TasksPage db={db}/> : active === 'Checkpoint Center' ? <CheckpointCenter db={db}/> : active === 'Handoff Center' ? <HandoffCenter db={db}/> : active === 'AI Command' ? <IntelligenceCenter db={db}/> : active === 'Settings' ? <SettingsPage voiceName={voiceName} speechRate={speechRate} onVoiceChange={setVoiceName} onSpeechRateChange={setSpeechRate} onOpenAiSettings={() => goTo('AI Command')}/> : active === 'Reminders' ? <RemindersPage db={db}/> : <div className="content"><div className="future-page"><span className="eyebrow">PLANNED CAPABILITY</span><h1>{active}</h1><p>This area is reserved for a later roadmap phase. No simulated data or functionality is shown here.</p></div></div>}
      </div>
      <nav className="levi-tabbar" aria-label="Groups">{navGroups.map((navGroup, index) => { const Icon = navGroup.icon; return <button key={navGroup.title} type="button" className={index === groupIndex ? 'is-on' : ''} aria-current={index === groupIndex ? 'page' : undefined} onClick={() => goTo(navGroup.items[0].label)} title={`${navGroup.title} (Alt+${index + 1})`}><Icon size={18}/><span>{navGroup.title}</span></button> })}</nav>
    </main>
    <GlobalAssistant activeSection={active} open={assistantOpen} size={assistantSize} onSizeChange={setAssistantSize} onOpenChange={setAssistantOpen} onOpenFull={() => { setAssistantOpen(false); goTo('AI Command') }} voiceName={voiceName} speechRate={speechRate}/>
    <ConfirmHost/>
  </div>
}

function GlobalAssistant({ activeSection, open, size, onSizeChange, onOpenChange, onOpenFull, voiceName, speechRate }: { activeSection: string; open: boolean; size: SheetSize; onSizeChange: (size: SheetSize) => void; onOpenChange: (open: boolean) => void; onOpenFull: () => void; voiceName: string; speechRate: number }) {
  const [configs, setConfigs] = useState<Partial<Record<AssistantSlot, AssistantConfig>>>({})
  const [conversationId, setConversationId] = useState('')
  const [messages, setMessages] = useState<AssistantMessage[]>([])
  const [proposals, setProposals] = useState<AssistantProposal[]>([])
  const [draft, setDraft] = useState('')
  const [busy, setBusy] = useState(false)
  const [, setResponseSequence] = useState(0)
  const [thought, setThought] = useState<{status:ThoughtLineStatus;startedAt:number;finishedAt?:number;activity:string;assistantMessageId?:string}|null>(null)
  const [activeRequestId, setActiveRequestId] = useState<string | null>(null)
  const cancelRequestedRef = useRef(false)
  const reducedMotion = useReducedMotion()
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>([])
  const [speaking, setSpeaking] = useState(false)

  const refreshConversation = useCallback(async (id: string): Promise<AssistantProposal[]> => {
    const [rows, actions] = await Promise.all([
      invoke<AssistantMessage[]>('get_ai_messages', { conversationId: id }),
      invoke<AssistantProposal[]>('get_ai_action_proposals', { conversationId: id }),
    ])
    setMessages(rows.slice(-12))
    setProposals(actions)
    return actions
  }, [])

  useEffect(() => {
    if (!open) return
    let cancelled = false
    void Promise.all([
      invoke<AssistantConfig>('get_ai_config'),
      invoke<AssistantConfig>('get_ai_secondary_config'),
      invoke<AssistantConversation[]>('list_ai_conversations'),
    ]).then(async ([primary, secondary, conversations]) => {
      if (cancelled) return
      setConfigs({ primary, secondary })
      let conversation = conversations[0]
      if (!conversation) conversation = await invoke<AssistantConversation>('create_ai_conversation')
      if (cancelled) return
      setConversationId(conversation.id)
      await refreshConversation(conversation.id)
    }).catch(value => { if (!cancelled) setError(errorMessage(value)) })
    return () => { cancelled = true }
  }, [open, refreshConversation])

  useEffect(() => {
    if (!open || !('speechSynthesis' in window)) return
    const update = () => setVoices(window.speechSynthesis.getVoices())
    update()
    window.speechSynthesis.addEventListener('voiceschanged', update)
    return () => window.speechSynthesis.removeEventListener('voiceschanged', update)
  }, [open])

  useEffect(() => () => { if ('speechSynthesis' in window) window.speechSynthesis.cancel() }, [])

  async function sendMessage(event: FormEvent) {
    event.preventDefault()
    const text = draft.trim()
    if (!conversationId || !text || busy) return
    const config = configs.primary
    if (!config?.hasApiKey || !config.model) { setError('Configure this provider profile in AI Command before sending.'); return }
    const startedAt=Date.now()
    const requestId=crypto.randomUUID()
    cancelRequestedRef.current=false
    setThought({status:'working',startedAt,activity:'Processing your question'})
    setActiveRequestId(requestId)
    setBusy(true); setError(''); setNotice('')
    try {
      const contextualText = `Current Command Center section: ${activeSection}. Use this only as interface context; workspace facts must come from LEVI's saved records.\n\n${text}`
      const response = await invoke<AssistantMessage>('send_ai_message', { input: { conversationId, message: contextualText, requestId, providerSlot: 'primary' } })
      setDraft('')
      const updatedProposals = await refreshConversation(conversationId)
      if (response.responseKind === 'error') {
        setError(response.content)
        const status:ThoughtLineStatus=cancelRequestedRef.current?'cancelled':response.content.includes('stopped at its output-token limit')?'incomplete':response.requestStatus==='outcome_unknown'?'unknown':'error'
        setThought({status,startedAt,finishedAt:Date.now(),activity:response.content,assistantMessageId:response.id})
      }
      else {
        setResponseSequence(value => value + 1)
        setThought({status:'complete',startedAt,finishedAt:Date.now(),activity:'The provider returned a complete response and LEVI saved it.',assistantMessageId:response.id})
        const proposalPending = updatedProposals.some(proposal => proposal.assistantMessageId === response.id && proposal.status === 'proposed')
        setNotice(proposalPending ? 'LEVI prepared an action proposal. Review and approve it in AI Command; nothing was executed here.' : 'Response saved to this local conversation.')
      }
    } catch (value) { setError(errorMessage(value)); await refreshConversation(conversationId).catch(() => undefined); setThought({status:'unknown',startedAt,finishedAt:Date.now(),activity:'The request command failed before LEVI could confirm its final provider outcome.'}) }
    finally { cancelRequestedRef.current=false; setBusy(false); setActiveRequestId(null) }
  }

  async function stopGeneration() {
    if (!activeRequestId) return
    setNotice('Stop requested. LEVI will report the provider outcome when the request settles.')
    try { await invoke('cancel_ai_request', { requestId: activeRequestId }); cancelRequestedRef.current=true }
    catch (value) { setError(errorMessage(value)) }
  }

  function speak(message: AssistantMessage) {
    if (!('speechSynthesis' in window)) { setError('Windows speech synthesis is unavailable in this WebView.'); return }
    window.speechSynthesis.cancel()
    const utterance = new SpeechSynthesisUtterance(message.content)
    const voice = voices.find(item => item.name === voiceName)
    if (voice) utterance.voice = voice
    utterance.rate = speechRate
    utterance.onstart = () => { setSpeaking(true); setNotice('Speaking with a local Windows voice. Audio is not sent by this playback control.') }
    utterance.onend = () => setSpeaking(false)
    utterance.onerror = () => { setSpeaking(false); setError('Speech playback failed or was interrupted.') }
    window.speechSynthesis.speak(utterance)
  }

  const config = configs.primary
  function draftPrompt(prompt: string) {
    setDraft(prompt)
    requestAnimationFrame(() => document.getElementById('global-assistant-input')?.focus())
  }
  const status = busy ? 'Working' : speaking ? 'Speaking with a local Windows voice' : config?.hasApiKey ? `${config.provider} · ${config.model || 'no model selected'}` : 'Set up a provider in AI Command'
  return <BottomSheet open={open} onClose={() => onOpenChange(false)} size={size} onSizeChange={onSizeChange} id="global-assistant-panel" className="jarvis-sheet"
    title={<><Bot size={16}/>JARVIS</>}
    headerExtra={<span className="jarvis-status">{status} · {activeSection}</span>}
    footer={<form className="global-assistant-composer" onSubmit={event => void sendMessage(event)}>
      <PromptBar simple id="global-assistant-input" value={draft} onChange={setDraft} placeholder={`Ask about ${activeSection.toLowerCase()} or anything in your workspace…`} disabled={busy} busy={busy} canSend={Boolean(!busy&&draft.trim()&&config?.hasApiKey&&config.model)} model={config?.model??''} onStop={()=>void stopGeneration()} helper="Enter to send · Shift+Enter for a new line"><ComposerActionMenu onDraftPrompt={draftPrompt}/></PromptBar>
    </form>}>
    <div className="global-assistant-messages" aria-live="polite">
      {!conversationId && !error && <p className="global-assistant-empty">Loading your saved conversation…</p>}
      {messages.map(message => <article key={message.id} className={`global-assistant-message ${message.role}`}>
        <div className="global-assistant-message-meta"><span>{message.role === 'user' ? 'You' : message.responseKind === 'live_ai' ? 'JARVIS' : message.responseKind.replaceAll('_', ' ')}</span>{message.model && <small>{message.model}</small>}{usageLabel(message) && <small title="Reported by the provider">{usageLabel(message)}</small>}</div>
        {thought?.assistantMessageId===message.id&&<ThoughtLine status={thought.status} startedAt={thought.startedAt} finishedAt={thought.finishedAt} activity={thought.activity} reducedMotion={reducedMotion}/>}<Suspense fallback={<p>Rendering response…</p>}><MarkdownMessage content={message.content}/></Suspense>
        {message.role === 'assistant' && message.responseKind === 'live_ai' && <button type="button" className="global-assistant-speak" onClick={() => speak(message)}><Volume2 size={13}/> Read aloud</button>}
      </article>)}
      {busy&&thought?.status==='working'&&<ThoughtLine status={thought.status} startedAt={thought.startedAt} activity={thought.activity} reducedMotion={reducedMotion}/>}
    </div>
    {(error || notice) && <div className={error ? 'global-assistant-error' : 'global-assistant-notice'} role={error ? 'alert' : 'status'}>{error || notice}</div>}
    {proposals.some(proposal => proposal.status === 'proposed') && <div className="global-assistant-approval"><strong>An action is waiting for your approval</strong><span>Review its exact details in AI Command before anything is created.</span><button type="button" className="button-primary" onClick={onOpenFull}>Review</button></div>}
    <div className="jarvis-footnote">{speaking && <button type="button" className="button-secondary" onClick={() => { window.speechSynthesis?.cancel(); setSpeaking(false) }}><Square size={12}/> Stop reading</button>}<button type="button" className="text-button" onClick={onOpenFull}>Open full AI Command</button><span>Text is sent only when you press Send. Read-aloud uses the local Windows voice.</span></div>
  </BottomSheet>
}

export default App
