export interface ConfirmOptions {
  title: string
  message: string
  /** Items the action affects, listed under the message. */
  details?: string[]
  confirmLabel?: string
  /** "danger" (the default) for archive, delete and anything that removes or releases something. */
  tone?: 'danger' | 'primary'
}

export type ConfirmRequest = ConfirmOptions & { resolve: (confirmed: boolean) => void }

let show: ((request: ConfirmRequest) => void) | null = null

/** Used by ConfirmHost to receive requests; returns a function that unregisters it. */
export function registerConfirmHost(handler: (request: ConfirmRequest) => void): () => void {
  show = handler
  return () => { if (show === handler) show = null }
}

/**
 * Asks for confirmation in LEVI's own bottom sheet instead of the Windows dialog.
 * Resolves true only when the person presses the confirm button.
 */
export function confirmAction(options: ConfirmOptions): Promise<boolean> {
  return new Promise(resolve => {
    if (show) show({ ...options, resolve })
    else resolve(window.confirm(options.message))
  })
}
