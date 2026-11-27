import { useEffect, useMemo, useRef, useState } from 'react'
import type Database from '@tauri-apps/plugin-sql'
import { invoke } from '@tauri-apps/api/core'
import { Archive, ArrowRight, Bot, CornerDownLeft, FolderKanban, Globe2, ListTodo, Search, Users } from 'lucide-react'
import { confirmAction } from './confirm'
import { errorMessage } from '../../lib/database'
import { getEcosystem, openLeviBrowser, openLeviOs } from '../ecosystem/ecosystem'

interface Result { id: string; group: string; title: string; detail?: string; icon: typeof Search; run: () => void | Promise<void>; secondary?: { label: string; run: () => void | Promise<void> } }
interface RecordRow { id: string; title: string; detail: string | null; kind: 'task' | 'project' | 'account' }

/**
 * Ctrl+K: jump to any section, run an action, or find a task, project or account.
 * Tasks also offer Archive (Ctrl+Enter), confirmed in the usual sheet.
 */
export function CommandPalette({ onClose, db, sections, onNavigate, onOpenJarvis, onToast }: {
  onClose: () => void; db: Database | null; sections: { label: string; group: string }[]
  onNavigate: (section: string) => void; onOpenJarvis: () => void; onToast: (text: string) => void
}) {
  const [query, setQuery] = useState('')
  const [records, setRecords] = useState<RecordRow[]>([])
  const [index, setIndex] = useState(0)
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => { requestAnimationFrame(() => input.current?.focus()) }, [])

  useEffect(() => {
    if (!db || query.trim().length < 2) return
    const like = `%${query.trim()}%`
    let live = true
    const timer = window.setTimeout(() => {
      void Promise.all([
        db.select<RecordRow[]>("SELECT t.id, t.title, COALESCE(p.name, '') || ' · ' || replace(t.status, '_', ' ') AS detail, 'task' AS kind FROM tasks t LEFT JOIN projects p ON p.id = t.project_id WHERE t.archived_at IS NULL AND t.title LIKE ? ORDER BY t.status = 'completed', t.priority DESC LIMIT 6", [like]),
        db.select<RecordRow[]>("SELECT id, name AS title, status AS detail, 'project' AS kind FROM projects WHERE archived_at IS NULL AND name LIKE ? LIMIT 4", [like]),
        db.select<RecordRow[]>("SELECT id, name AS title, status AS detail, 'account' AS kind FROM accounts WHERE archived_at IS NULL AND (name LIKE ? OR COALESCE(identifier, '') LIKE ?) LIMIT 4", [like, like]),
      ]).then(groups => { if (live) setRecords(groups.flat()) }).catch(() => undefined)
    }, 120)
    return () => { live = false; window.clearTimeout(timer) }
  }, [db, query])

  const results = useMemo<Result[]>(() => {
    const q = query.trim().toLowerCase()
    const matches = (text: string) => !q || text.toLowerCase().includes(q)
    const go = (section: string) => () => { onClose(); onNavigate(section) }
    const actions: Result[] = [
      { id: 'a-jarvis', group: 'Actions', title: 'Ask JARVIS', detail: 'Alt+Shift+L', icon: Bot, run: () => { onClose(); onOpenJarvis() } },
      { id: 'a-new-task', group: 'Actions', title: 'Add a task', detail: 'Opens Tasks', icon: ListTodo, run: go('Tasks') },
      { id: 'a-levi-os', group: 'Actions', title: 'Open LEVI OS', detail: 'Missions and habits', icon: Globe2, run: async () => { onClose(); try { await openLeviOs(await getEcosystem()) } catch (e) { onToast(errorMessage(e)) } } },
      { id: 'a-browser', group: 'Actions', title: 'Open LEVI Browser', icon: Globe2, run: async () => { onClose(); try { await openLeviBrowser() } catch (e) { onToast(errorMessage(e)) } } },
    ].filter(item => matches(item.title))
    const sectionResults: Result[] = sections.filter(item => matches(`${item.label} ${item.group}`)).map(item => ({ id: `s-${item.label}`, group: 'Go to', title: item.label, detail: item.group, icon: ArrowRight, run: go(item.label) }))
    const recordResults: Result[] = (q.length >= 2 ? records : []).map(row => row.kind === 'task'
      ? { id: `t-${row.id}`, group: 'Tasks', title: row.title, detail: row.detail ?? undefined, icon: ListTodo, run: go('Tasks'), secondary: { label: 'Archive', run: async () => {
          onClose()
          if (!(await confirmAction({ title: `Archive “${row.title}”?`, message: 'The task moves to the archive. Its history is kept.', confirmLabel: 'Archive task' }))) return
          try { await invoke('archive_task', { taskId: row.id }); onToast(`“${row.title}” archived.`) } catch (e) { onToast(errorMessage(e)) }
        } } }
      : row.kind === 'project' ? { id: `p-${row.id}`, group: 'Projects', title: row.title, detail: row.detail ?? undefined, icon: FolderKanban, run: go('Projects') }
      : { id: `c-${row.id}`, group: 'Accounts', title: row.title, detail: row.detail ?? undefined, icon: Users, run: go('Accounts') })
    return q ? [...recordResults, ...sectionResults, ...actions] : [...actions, ...sectionResults]
  }, [query, sections, records, onClose, onNavigate, onOpenJarvis, onToast])


  function key(event: React.KeyboardEvent) {
    if (event.key === 'ArrowDown') { event.preventDefault(); setIndex(i => Math.min(results.length - 1, i + 1)) }
    else if (event.key === 'ArrowUp') { event.preventDefault(); setIndex(i => Math.max(0, i - 1)) }
    else if (event.key === 'Enter') { event.preventDefault(); const item = results[index]; if (!item) return; void (event.ctrlKey && item.secondary ? item.secondary.run() : item.run()) }
    else if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); onClose() }
  }

  let lastGroup = ''
  return <>
    <button type="button" className="levi-sheet-scrim" aria-label="Close" tabIndex={-1} onClick={onClose}/>
    <div className="palette" role="dialog" aria-modal="true" aria-label="Command palette" onKeyDown={key}>
      <label className="palette-input"><Search size={16}/><input ref={input} value={query} onChange={event => { setQuery(event.target.value); setIndex(0) }} placeholder="Go to a section, run an action, or find a task, project or account" aria-label="Search" aria-controls="palette-results" aria-activedescendant={results[index] ? `palette-${results[index].id}` : undefined}/><kbd>Esc</kbd></label>
      <ul className="palette-results" id="palette-results" role="listbox">
        {results.length === 0 && <li className="palette-empty">Nothing matches “{query}”.</li>}
        {results.map((item, i) => {
          const header = item.group !== lastGroup ? item.group : null
          lastGroup = item.group
          const Icon = item.icon
          return <li key={item.id} role="presentation">
            {header && <div className="palette-group">{header}</div>}
            <div id={`palette-${item.id}`} role="option" aria-selected={i === index} className={`palette-row ${i === index ? 'is-active' : ''}`} onMouseEnter={() => setIndex(i)} onClick={() => void item.run()}>
              <Icon size={15}/><span className="palette-title">{item.title}</span>{item.detail && <span className="palette-detail">{item.detail}</span>}
              {item.secondary && <button type="button" className="palette-secondary" onClick={event => { event.stopPropagation(); void item.secondary!.run() }} title="Ctrl+Enter"><Archive size={12}/>{item.secondary.label}</button>}
              {i === index && <CornerDownLeft size={13} className="palette-enter"/>}
            </div>
          </li>
        })}
      </ul>
      <footer className="palette-foot"><span>↑↓ move</span><span>Enter open</span><span>Ctrl+Enter archive task</span></footer>
    </div>
  </>
}
