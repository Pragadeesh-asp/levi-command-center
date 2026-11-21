import { useEffect, useRef, useState } from 'react'
import { LayoutGrid } from 'lucide-react'
import { errorMessage } from '../../lib/database'
import { getEcosystem, goToSettings, hostOf, openLeviBrowser, openLeviOs, type EcosystemStatus } from './ecosystem'
import './ecosystem.css'

/** Top-bar switcher for the three LEVI apps, with each app's live state. */
export function AppSwitcher() {
  const [open, setOpen] = useState(false)
  const [status, setStatus] = useState<EcosystemStatus | null>(null)
  const [message, setMessage] = useState('')
  const rootRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    void getEcosystem().then(setStatus).catch(error => setMessage(errorMessage(error)))
    const onPointer = (event: PointerEvent) => { if (!rootRef.current?.contains(event.target as Node)) setOpen(false) }
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') setOpen(false) }
    document.addEventListener('pointerdown', onPointer)
    document.addEventListener('keydown', onKey)
    return () => { document.removeEventListener('pointerdown', onPointer); document.removeEventListener('keydown', onKey) }
  }, [open])

  async function run(action: () => Promise<unknown>, done: string) {
    try { await action(); setMessage(done) } catch (error) { setMessage(errorMessage(error)) }
  }

  const browser = status?.browser
  const browserLine = !browser ? 'Checking…' : browser.launchable ? `Version ${browser.version} · ${browser.spaces.length} Space${browser.spaces.length === 1 ? '' : 's'}` : browser.problem ?? 'Not available'
  const osReady = Boolean(status?.leviOsUrl && status?.leviOsSpace)
  const osLine = !status ? 'Checking…' : status.leviOsUrl ? `${hostOf(status.leviOsUrl)}${status.leviOsSpace ? '' : ' · choose a Space in Settings'}` : 'Not set up yet'

  return <div className="app-switcher" ref={rootRef}>
    <button type="button" className="icon-button app-switcher-button" aria-label="LEVI apps" aria-haspopup="dialog" aria-expanded={open} onClick={() => { setMessage(''); setOpen(value => !value) }}><LayoutGrid size={17}/></button>
    {open && <div className="app-switcher-panel" role="dialog" aria-label="LEVI apps">
      <span className="eyebrow">LEVI APPS</span>
      <div className="app-tile current"><span className="app-tile-mark cc" aria-hidden="true">C</span><div><strong>Command Center</strong><small>This app</small></div><span className="app-tile-here">Here</span></div>
      <div className="app-tile"><span className="app-tile-mark browser" aria-hidden="true">B</span><div><strong>LEVI Browser</strong><small>{browserLine}</small></div>
        <button type="button" className="button-secondary" disabled={!browser?.launchable} onClick={() => void run(openLeviBrowser, 'LEVI Browser was asked to come forward.')}>Open</button></div>
      <div className="app-tile"><span className="app-tile-mark os" aria-hidden="true">O</span><div><strong>LEVI OS</strong><small>{osLine}</small></div>
        {osReady && status
          ? <button type="button" className="button-secondary" disabled={!browser?.launchable} onClick={() => void run(() => openLeviOs(status), 'LEVI OS is opening in LEVI Browser.')}>Open</button>
          : <button type="button" className="button-secondary" onClick={() => { setOpen(false); goToSettings() }}>Set up</button>}
      </div>
      {message && <p className="app-switcher-message" role="status">{message}</p>}
    </div>}
  </div>
}
