import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { Network } from 'lucide-react'
import { errorMessage } from '../../lib/database'
import { getEcosystem, openLeviOs, saveLeviOs, type EcosystemStatus } from './ecosystem'
import './ecosystem.css'

/** Settings card that connects Command Center with LEVI Browser and LEVI OS. */
export function EcosystemSettings() {
  const [status, setStatus] = useState<EcosystemStatus | null>(null)
  const [url, setUrl] = useState('')
  const [space, setSpace] = useState('')
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  const [error, setError] = useState('')

  const apply = useCallback((next: EcosystemStatus) => { setStatus(next); setUrl(next.leviOsUrl ?? ''); setSpace(next.leviOsSpace ?? '') }, [])
  useEffect(() => { void getEcosystem().then(apply).catch(cause => setError(errorMessage(cause))) }, [apply])

  async function save(event: FormEvent) {
    event.preventDefault(); setBusy(true); setError(''); setMessage('')
    try { apply(await saveLeviOs(url, space)); setMessage('Saved.') } catch (cause) { setError(errorMessage(cause)) } finally { setBusy(false) }
  }

  async function openNow() {
    if (!status) return
    setBusy(true); setError(''); setMessage('')
    try { const result = await openLeviOs(status); setMessage(`Asked LEVI Browser to open ${result.host} in ${result.spaceName}.`) } catch (cause) { setError(errorMessage(cause)) } finally { setBusy(false) }
  }

  const browser = status?.browser
  const dirty = Boolean(status) && (url !== (status?.leviOsUrl ?? '') || space !== (status?.leviOsSpace ?? ''))

  return <section className="panel settings-card ecosystem-card">
    <div className="settings-card-title"><span className="settings-icon"><Network size={17}/></span><div><h2>LEVI ecosystem</h2><p>Connect Command Center with LEVI Browser and LEVI OS.</p></div></div>
    <div className="ecosystem-browser" aria-live="polite">
      <strong>LEVI Browser</strong>
      {!browser ? <span>Checking…</span> : browser.launchable
        ? <span>Version {browser.version} · {browser.spaces.length} Space{browser.spaces.length === 1 ? '' : 's'}</span>
        : <span className="ecosystem-problem">{browser.problem}</span>}
    </div>
    <form className="ecosystem-form" onSubmit={event => void save(event)}>
      <label className="settings-control" htmlFor="levi-os-url">LEVI OS address
        <input id="levi-os-url" type="url" inputMode="url" placeholder="https://your-levi-os.example.app" value={url} onChange={event => setUrl(event.target.value)} disabled={busy}/>
      </label>
      <label className="settings-control" htmlFor="levi-os-space">Space that opens LEVI OS
        <select id="levi-os-space" value={space} onChange={event => setSpace(event.target.value)} disabled={busy || !browser?.spaces.length}>
          <option value="">{browser?.spaces.length ? 'Choose a Space' : 'No Spaces found'}</option>
          {browser?.spaces.map(item => <option key={item.id} value={item.id}>{item.avatar} · {item.name}</option>)}
        </select>
      </label>
      <div className="ecosystem-actions">
        <button type="submit" className="button-secondary" disabled={busy || !dirty}>Save</button>
        <button type="button" className="button-secondary" disabled={busy || dirty || !status?.leviOsUrl || !status?.leviOsSpace || !browser?.launchable} onClick={() => void openNow()}>Open LEVI OS</button>
      </div>
    </form>
    <small>Use https, or http://localhost while developing. LEVI OS keeps its own sign-in inside that Space.</small>
    {message && <small role="status">{message}</small>}
    {error && <div className="inline-error" role="alert">{error}</div>}
  </section>
}
