import { useEffect, useState } from 'react'
import { Check, Pause, Play, X } from 'lucide-react'

const PRESETS = [25, 50, 90]

function format(seconds: number) {
  const m = Math.floor(seconds / 60)
  const s = seconds % 60
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}

/**
 * One task, full screen, with a countdown. Everything else is hidden until it ends or you
 * leave. "Done" marks the task completed through the normal save path.
 */
export function FocusMode({ task, onExit, onComplete }: { task: { id: string; title: string; project_name: string | null; blocker?: string }; onExit: () => void; onComplete: () => Promise<void> }) {
  const [minutes, setMinutes] = useState(25)
  const [left, setLeft] = useState(25 * 60)
  const [running, setRunning] = useState(false)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!running) return
    const timer = window.setInterval(() => setLeft(value => {
      if (value <= 1) { setRunning(false); return 0 }
      return value - 1
    }), 1000)
    return () => window.clearInterval(timer)
  }, [running])

  useEffect(() => {
    document.title = running ? `${format(left)} · ${task.title}` : 'LEVI COMMAND CENTER'
    return () => { document.title = 'LEVI COMMAND CENTER' }
  }, [left, running, task.title])

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopImmediatePropagation(); onExit() }
      if (event.key === ' ' && !(event.target instanceof HTMLButtonElement)) { event.preventDefault(); setRunning(value => !value) }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [onExit])

  const progress = 1 - left / (minutes * 60)
  return <div className="focus-mode" role="dialog" aria-modal="true" aria-label={`Focus on ${task.title}`}>
    <button type="button" className="focus-exit" onClick={onExit} aria-label="Leave focus mode (Esc)"><X size={18}/></button>
    <div className="focus-body">
      <p className="focus-eyebrow">FOCUS{task.project_name ? ` · ${task.project_name}` : ''}</p>
      <h1>{task.title}</h1>
      <div className="focus-ring" style={{ ['--p' as string]: String(progress) }} role="timer" aria-live="off" aria-label={`${format(left)} left`}>
        <span>{left === 0 ? 'Time' : format(left)}</span>
      </div>
      {!running && left === minutes * 60 && <div className="focus-presets" role="group" aria-label="Length">
        {PRESETS.map(value => <button key={value} type="button" className={minutes === value ? 'is-on' : ''} onClick={() => { setMinutes(value); setLeft(value * 60) }}>{value} min</button>)}
      </div>}
      <div className="focus-actions">
        <button type="button" className="button-secondary" onClick={() => setRunning(value => !value)} disabled={left === 0}>{running ? <><Pause size={15}/> Pause</> : <><Play size={15}/> {left === minutes * 60 ? 'Start' : 'Resume'}</>}</button>
        <button type="button" className="button-primary" disabled={busy} onClick={async () => { setBusy(true); try { await onComplete() } finally { setBusy(false) } }}><Check size={15}/> Mark done</button>
      </div>
      <p className="focus-hint">Space starts or pauses · Esc leaves</p>
    </div>
  </div>
}
