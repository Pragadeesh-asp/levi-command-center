import { useEffect, useRef, useState } from 'react'
import { Globe2 } from 'lucide-react'
import { errorMessage } from '../../lib/database'
import { getEcosystem, openInSpace, type Space } from './ecosystem'
import './ecosystem.css'

interface Props { accountId: string; accountName: string; url: string | null; onResult: (message: string) => void }

/** Opens an account's conversation in a LEVI Browser Space the user picks. */
export function OpenInSpaceButton({ accountId, accountName, url, onResult }: Props) {
  const [open, setOpen] = useState(false)
  const [spaces, setSpaces] = useState<Space[] | null>(null)
  const [problem, setProblem] = useState('')
  const rootRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    void getEcosystem().then(status => {
      if (!status.browser.launchable) setProblem(status.browser.problem ?? 'LEVI Browser is not available.')
      else setSpaces(status.browser.spaces)
    }).catch(error => setProblem(errorMessage(error)))
    const onPointer = (event: PointerEvent) => { if (!rootRef.current?.contains(event.target as Node)) setOpen(false) }
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') setOpen(false) }
    document.addEventListener('pointerdown', onPointer)
    document.addEventListener('keydown', onKey)
    return () => { document.removeEventListener('pointerdown', onPointer); document.removeEventListener('keydown', onKey) }
  }, [open])

  async function choose(space: Space) {
    if (!url) return
    setOpen(false)
    try { const result = await openInSpace(space.id, url, accountId); onResult(`Asked LEVI Browser to open ${result.host} in ${result.spaceName} for ${accountName}.`) }
    catch (error) { onResult(errorMessage(error)) }
  }

  return <div className="space-picker" ref={rootRef}>
    <button type="button" className="button-open space-open" disabled={!url} aria-haspopup="menu" aria-expanded={open}
      title={url ? `Open ${accountName} in a LEVI Browser Space` : 'Add a conversation URL to open this account in a Space'}
      onClick={() => { if (!open) { setSpaces(null); setProblem('') } setOpen(value => !value) }}><Globe2 size={14}/> SPACE</button>
    {open && <div className="space-menu" role="menu" aria-label={`Open ${accountName} in`}>
      {problem ? <p>{problem}</p>
        : spaces === null ? <p>Loading Spaces…</p>
        : spaces.length === 0 ? <p>No Spaces yet. Add one in LEVI Browser.</p>
        : spaces.map(space => <button type="button" role="menuitem" key={space.id} onClick={() => void choose(space)}><i style={{ background: space.accent }} aria-hidden="true">{space.avatar}</i>{space.name}</button>)}
    </div>}
  </div>
}
