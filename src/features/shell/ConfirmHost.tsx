import { useEffect, useState } from 'react'
import { BottomSheet } from './BottomSheet'
import { registerConfirmHost, type ConfirmRequest } from './confirm'

/** Mount once near the root; it shows whichever confirmation confirmAction() asked for. */
export function ConfirmHost() {
  const [request, setRequest] = useState<ConfirmRequest | null>(null)

  useEffect(() => registerConfirmHost(next => setRequest(current => { current?.resolve(false); return next })), [])

  useEffect(() => {
    if (!request) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopImmediatePropagation()
      request.resolve(false)
      setRequest(null)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [request])

  function finish(confirmed: boolean) {
    request?.resolve(confirmed)
    setRequest(null)
  }

  const danger = request?.tone !== 'primary'
  return <BottomSheet open={Boolean(request)} onClose={() => finish(false)} title={request?.title ?? ''} role="alertdialog" className="levi-confirm"
    footer={<>
      <button type="button" className="button-secondary" data-autofocus={danger ? '' : undefined} onClick={() => finish(false)}>Cancel</button>
      <button type="button" className={danger ? 'button-danger-solid' : 'button-primary'} data-autofocus={danger ? undefined : ''} onClick={() => finish(true)}>{request?.confirmLabel ?? 'Confirm'}</button>
    </>}>
    <p className="levi-confirm-message">{request?.message}</p>
    {request?.details && request.details.length > 0 && <ul className="levi-confirm-details">{request.details.map(item => <li key={item}>{item}</li>)}</ul>}
  </BottomSheet>
}
