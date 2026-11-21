import { useEffect, useRef, useState } from 'react'
import { Liquid } from 'liquid-gooey'
import { ListTodo, Plus, Sparkles, Workflow } from 'lucide-react'

interface ComposerActionMenuProps {
  onLocalSummary?: () => void
  onDraftPrompt: (prompt: string) => void
  onPrepareHandoff?: () => void
}

export function ComposerActionMenu({ onLocalSummary, onDraftPrompt, onPrepareHandoff }: ComposerActionMenuProps) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const toggleRef = useRef<HTMLButtonElement>(null)
  const actions = [
    ...(onLocalSummary ? [{ label: 'Add local workforce summary', icon: Sparkles, run: onLocalSummary }] : []),
    { label: 'Draft a question about blocked tasks', icon: ListTodo, run: () => onDraftPrompt('Which tasks are blocked, and what dependencies or file claims block them?') },
    ...(onPrepareHandoff ? [{ label: 'Prepare a task handoff', icon: Workflow, run: onPrepareHandoff }] : []),
  ]

  useEffect(() => {
    if (!open) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        setOpen(false)
        toggleRef.current?.focus()
      }
    }
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false)
    }
    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('pointerdown', onPointerDown)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('pointerdown', onPointerDown)
    }
  }, [open])

  return <div ref={rootRef} className={`composer-action-menu${open ? ' is-open' : ''}`}>
    <Liquid className="composer-action-liquid" blur={6} contrast={18} fill="rgba(22,70,160,.5)" shadow="0 5px 16px rgba(0,0,0,.3)">
      <Liquid.Item x={0} y={0} transition="bouncy" className="composer-action-item composer-action-toggle">
        <button ref={toggleRef} type="button" aria-label={open ? 'Close composer actions' : 'Open composer actions'} aria-expanded={open} onClick={() => setOpen(value => !value)}>
          {open ? <Plus size={18} className="composer-action-close"/> : <Plus size={18}/>}
        </button>
      </Liquid.Item>
      {actions.map(({ label, icon: Icon, run }, index) => <Liquid.Item key={label} x={open ? 44 * (index + 1) : 0} y={0} transition="bouncy" delay={open ? index * 35 : 0} className="composer-action-item">
        <button type="button" aria-label={label} title={label} tabIndex={open ? 0 : -1} aria-hidden={!open} onClick={() => { setOpen(false); run() }}>
          <Icon size={16}/>
        </button>
      </Liquid.Item>)}
    </Liquid>
  </div>
}
