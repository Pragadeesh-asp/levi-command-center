import { useCallback, useEffect, useState } from 'react'
import type Database from '@tauri-apps/plugin-sql'
import { errorMessage } from '../../lib/database'

interface SeatRow { id: string; name: string; status: string; last_accessed_at: string | null; detected_at: string | null; reset_at: string | null; reset_status: string | null }

/** SQLite's datetime('now') is UTC without a zone marker. */
const fromSqliteUtc = (value: string) => new Date(`${value.replace(' ', 'T')}Z`)
/** Reset times are typed into a date-time field, so they are local time unless they say otherwise. */
const fromReset = (value: string) => new Date(/[zZ]|[+-]\d\d:?\d\d$/.test(value) ? value : value.replace(' ', 'T'))

function countdown(ms: number) {
  const minutes = Math.max(0, Math.round(ms / 60000))
  const h = Math.floor(minutes / 60)
  return h ? `${h}h ${String(minutes % 60).padStart(2, '0')}m` : `${minutes}m`
}

/**
 * One gauge per Claude account, from the usage-limit events Command Center recorded.
 * Claude doesn't publish how much of a limit is used, so the meter shows what is known:
 * free, limited until a reset time (ring empties as the reset approaches), or limited with
 * the reset time unknown.
 */
export function SeatMeter({ db, onOpenAccounts }: { db: Database; onOpenAccounts?: () => void }) {
  const [seats, setSeats] = useState<SeatRow[] | null>(null)
  const [error, setError] = useState('')
  const [now, setNow] = useState(() => Date.now())

  const load = useCallback(async () => {
    const rows = await db.select<SeatRow[]>(`SELECT a.id, a.name, a.status, a.last_accessed_at, e.detected_at, e.reset_at, e.reset_status
      FROM accounts a
      LEFT JOIN usage_limit_events e ON e.id = (SELECT id FROM usage_limit_events WHERE account_id = a.id AND status = 'waiting' ORDER BY detected_at DESC LIMIT 1)
      WHERE a.archived_at IS NULL ORDER BY a.name`)
    setSeats(rows)
  }, [db])

  useEffect(() => {
    // oxlint-disable-next-line react/set-state-in-effect -- loading saved limit events when the meter appears
    void load().catch(e => setError(errorMessage(e)))
    const tick = window.setInterval(() => setNow(Date.now()), 30000)
    const refresh = window.setInterval(() => void load().catch(() => undefined), 120000)
    return () => { window.clearInterval(tick); window.clearInterval(refresh) }
  }, [load])

  if (error) return <div className="inline-error" role="alert">Seat meter: {error}</div>
  if (!seats) return <section className="seat-meter" aria-busy="true"><div className="skeleton seat-skeleton"/><div className="skeleton seat-skeleton"/><div className="skeleton seat-skeleton"/></section>
  if (!seats.length) return null

  const free = seats.filter(seat => !seat.detected_at || (seat.reset_at && fromReset(seat.reset_at).getTime() <= now)).length
  return <section className="seat-meter" aria-label="Claude account availability">
    <header className="seat-meter-head"><h2>Seats</h2><span>{free} of {seats.length} free</span>{onOpenAccounts && <button type="button" className="text-button" onClick={onOpenAccounts}>Accounts</button>}</header>
    <div className="seat-list">
      {seats.map(seat => {
        const limited = Boolean(seat.detected_at)
        const reset = seat.reset_at ? fromReset(seat.reset_at) : null
        const detected = seat.detected_at ? fromSqliteUtc(seat.detected_at) : null
        const resetPassed = Boolean(reset && reset.getTime() <= now)
        const state = !limited || resetPassed ? 'free' : reset ? 'limited' : 'unknown'
        const remaining = reset && detected ? Math.min(1, Math.max(0, (reset.getTime() - now) / Math.max(1, reset.getTime() - detected.getTime()))) : 1
        const fill = state === 'free' ? 1 : state === 'limited' ? 1 - remaining : 0
        const caption = state === 'free'
          ? (resetPassed ? 'Reset time passed; confirm resume in Checkpoints' : 'Free')
          : state === 'limited' ? `Limited · resets in ${countdown(reset!.getTime() - now)} (${reset!.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })})` : 'Limited · reset time unknown'
        return <article key={seat.id} className={`seat seat-${state}`} title={caption}>
          <svg className="seat-ring" viewBox="0 0 36 36" aria-hidden="true"><circle cx="18" cy="18" r="15" className="seat-track"/><circle cx="18" cy="18" r="15" className="seat-fill" pathLength="100" strokeDasharray={`${Math.round(fill * 100)} 100`}/></svg>
          <div><strong>{seat.name}</strong><span>{caption}</span></div>
        </article>
      })}
    </div>
  </section>
}
