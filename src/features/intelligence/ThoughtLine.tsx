import { useEffect, useState } from 'react'
import { Sparkles } from 'lucide-react'

export type ThoughtLineStatus = 'working' | 'complete' | 'error' | 'incomplete' | 'cancelled' | 'unknown'

interface ThoughtLineProps {
  status: ThoughtLineStatus
  startedAt: number
  activity: string
  reducedMotion: boolean
  finishedAt?: number
}

const statusLabels: Record<Exclude<ThoughtLineStatus, 'working'>, string> = {
  complete: 'Thought for',
  error: 'Request failed',
  incomplete: 'Response incomplete · not presented as complete',
  cancelled: 'Stopped · provider outcome unknown',
  unknown: 'Outcome unknown · not reported as complete',
}

/** A textual activity trace. It reports lifecycle states supplied by the caller; it never invents tool steps. */
export function ThoughtLine({ status, startedAt, activity, reducedMotion, finishedAt }: ThoughtLineProps) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (status !== 'working') return
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [status])

  const elapsed = Math.max(0, (finishedAt ?? now) - startedAt)
  const seconds = (elapsed / 1000).toFixed(1)
  const settledLabel = status === 'complete' ? `${statusLabels.complete} ${seconds}s` : statusLabels[status as Exclude<ThoughtLineStatus, 'working'>]
  return <div className={`thought-line thought-line-${status}${reducedMotion ? ' motion-reduced' : ''}`} role="status" aria-live="polite" aria-atomic="true">
    <Sparkles size={15} aria-hidden="true" />
    <span className="thought-line-copy">
      <strong>{status === 'working' ? activity : settledLabel}</strong>
      {status !== 'working' && status !== 'complete' && <small>{activity}</small>}
      {status === 'working' && <small>{seconds}s</small>}
    </span>
    {status === 'working' && <span className="thought-line-trace" aria-hidden="true"><i /></span>}
    {status === 'complete' && <details className="thought-line-details"><summary>Activity detail</summary><span>{activity}</span></details>}
  </div>
}
