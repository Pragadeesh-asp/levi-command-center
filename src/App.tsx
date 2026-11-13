import { useState } from 'react'
import { Activity, AlertCircle, Archive, ArrowDownRight, ArrowUpRight, Bell, CalendarDays, Check, ChevronDown, CircleHelp, Command, FolderKanban, LayoutDashboard, ListTodo, Plus, Search, Settings, Users, Zap } from 'lucide-react'
import './App.css'

const nav = [
  { label: 'Overview', icon: LayoutDashboard }, { label: 'Accounts', icon: Users }, { label: 'Projects', icon: FolderKanban },
  { label: 'Tasks', icon: ListTodo }, { label: 'Calendar', icon: CalendarDays }, { label: 'Reminders', icon: Bell },
  { label: 'Activity', icon: Activity }, { label: 'Output Vault', icon: Archive },
]
const metrics = [
  { label: 'Total accounts', value: '—', detail: 'Register your first account', icon: Users, tone: 'blue' },
  { label: 'Active projects', value: '—', detail: 'Your workspaces at a glance', icon: FolderKanban, tone: 'cyan' },
  { label: 'Open tasks', value: '—', detail: 'Nothing waiting in the queue', icon: ListTodo, tone: 'violet' },
  { label: 'Overdue', value: '—', detail: 'Deadlines that need attention', icon: AlertCircle, tone: 'amber' },
]

function App() {
  const [active, setActive] = useState('Overview')
  const [query, setQuery] = useState('')
  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><div className="brand-mark"><Command size={18}/></div><div><strong>COMMAND CENTER</strong><span>AI WORKFORCE OS</span></div></div>
      <div className="workspace-label">WORKSPACE <ChevronDown size={13}/></div>
      <div className="workspace"><div className="workspace-avatar">P</div><div><strong>Personal workspace</strong><small>Local environment</small></div><span className="local-dot"/></div>
      <div className="nav-caption">OPERATIONS</div>
      <nav>{nav.map(({label, icon: Icon}) => <button key={label} className={`nav-item ${active === label ? 'selected' : ''}`} onClick={() => setActive(label)}><Icon size={17}/><span>{label}</span></button>)}</nav>
      <div className="sidebar-bottom"><button className={`nav-item ${active === 'Analytics' ? 'selected' : ''}`} onClick={() => setActive('Analytics')}><Activity size={17}/><span>Analytics</span></button><button className={`nav-item ${active === 'Settings' ? 'selected' : ''}`} onClick={() => setActive('Settings')}><Settings size={17}/><span>Settings</span></button><div className="help-card"><div className="help-icon"><CircleHelp size={16}/></div><strong>Need a hand?</strong><p>Get started with your command center.</p><button onClick={() => setActive('Settings')}>View setup guide <ArrowUpRight size={13}/></button></div><div className="build-label"><span className="online-dot"/> LOCAL SYSTEM <span>v0.1.0</span></div></div>
    </aside>
    <main className="main-area">
      <header className="topbar"><div className="breadcrumb">Command Center <span>/</span> <strong>{active}</strong></div><div className="top-actions"><label className="searchbox"><Search size={15}/><input value={query} onChange={e => setQuery(e.target.value)} placeholder="Search anything..."/><kbd>⌘ K</kbd></label><button className="icon-button" aria-label="Notifications"><Bell size={17}/><i/></button><div className="avatar">P</div></div></header>
      <div className="content"><div className="page-heading"><div><div className="eyebrow"><span className="live-pulse"/> YOUR OPERATIONS, AT A GLANCE</div><h1>Good afternoon<span className="heading-period">.</span></h1><p className="subtitle">One control room. Every AI account. Total clarity.</p></div><div className="heading-actions"><button className="button-secondary"><CalendarDays size={15}/> Today <ChevronDown size={14}/></button><button className="button-primary" onClick={() => setActive('Accounts')}><Plus size={16}/> Add account</button></div></div>
      <section className="metrics-grid">{metrics.map(({label,value,detail,icon:Icon,tone})=><article className="metric-card" key={label}><div className="metric-top"><span>{label}</span><div className={`metric-icon ${tone}`}><Icon size={16}/></div></div><div className="metric-value">{value}</div><div className="metric-detail">{detail}</div><div className={`metric-trend ${tone}`}><ArrowDownRight size={13}/> Waiting for data</div></article>)}</section>
      <section className="overview-grid"><article className="panel activity-panel"><div className="panel-heading"><div><h2>Account activity</h2><p>A live view of your AI workforce</p></div><button className="text-button" onClick={() => setActive('Accounts')}>View accounts <ArrowUpRight size={14}/></button></div><div className="chart-placeholder"><div className="chart-y"><span>10</span><span>8</span><span>6</span><span>4</span><span>2</span><span>0</span></div><div className="chart-area"><div className="chart-empty"><div className="chart-orb"><Activity size={20}/></div><strong>Your activity will appear here</strong><span>Once you add accounts, this panel will show their status over time.</span></div><div className="chart-x"><span>MON</span><span>TUE</span><span>WED</span><span>THU</span><span>FRI</span><span>SAT</span><span>SUN</span></div></div></div><div className="chart-legend"><span><i className="legend-dot cyan-dot"/> Working</span><span><i className="legend-dot blue-dot"/> Waiting</span><span><i className="legend-dot grey-dot"/> Idle</span><span className="chart-period">LAST 7 DAYS <ChevronDown size={12}/></span></div></article>
        <article className="panel quick-panel"><div className="panel-heading"><div><h2>Quick launch</h2><p>Jump back into your workflow</p></div><Zap size={16} className="quick-zap"/></div><div className="quick-empty"><div className="quick-icon"><Users size={19}/></div><strong>No accounts yet</strong><span>Add an account to launch its browser profile in one click.</span><button className="quick-add" onClick={() => setActive('Accounts')}><Plus size={14}/> Register account</button></div><div className="quick-foot"><span>ACCOUNT REGISTRY</span><span>READY WHEN YOU ARE</span></div></article></section>
      <section className="bottom-grid"><article className="panel list-panel"><div className="panel-heading"><div><h2>Priority tasks</h2><p>What needs your attention next</p></div><button className="text-button" onClick={() => setActive('Tasks')}>All tasks <ArrowUpRight size={14}/></button></div><div className="list-empty"><div className="empty-check"><Check size={16}/></div><div><strong>Clear runway</strong><span>Priority tasks you create will show up here.</span></div></div></article><article className="panel list-panel"><div className="panel-heading"><div><h2>Upcoming deadlines</h2><p>Stay ahead of the next milestone</p></div><button className="text-button" onClick={() => setActive('Calendar')}>Calendar <ArrowUpRight size={14}/></button></div><div className="list-empty"><div className="empty-calendar"><CalendarDays size={16}/></div><div><strong>No deadlines coming up</strong><span>Project and task deadlines will appear here.</span></div></div></article></section>
      <footer><span><span className="online-dot"/> All data stored locally on this device</span><span>CLAUDE COMMAND CENTER <i>•</i> BUILT FOR CLARITY <ArrowUpRight size={11}/></span></footer>
      </div>
    </main>
  </div>
}

export default App
