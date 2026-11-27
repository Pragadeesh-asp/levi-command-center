import { useEffect, useRef, type PointerEvent, type ReactNode } from 'react'
import { X } from 'lucide-react'

export type SheetSize = 'auto' | 'half' | 'full'

interface BottomSheetProps {
  open: boolean
  onClose: () => void
  title: ReactNode
  children: ReactNode
  footer?: ReactNode
  size?: SheetSize
  /** When given, the sheet offers Half / Full and can be dragged up to full height. */
  onSizeChange?: (size: SheetSize) => void
  headerExtra?: ReactNode
  role?: 'dialog' | 'alertdialog'
  id?: string
  className?: string
}

/**
 * A sheet that slides up from the bottom of the window. Drag its handle down to close
 * it, or up to make it full height; Escape is handled by whoever owns the sheet.
 */
export function BottomSheet({ open, onClose, title, children, footer, size = 'auto', onSizeChange, headerExtra, role = 'dialog', id, className = '' }: BottomSheetProps) {
  const sheet = useRef<HTMLElement>(null)
  const drag = useRef<number | null>(null)

  useEffect(() => {
    if (!open) return
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const timer = window.setTimeout(() => {
      sheet.current?.querySelector<HTMLElement>('[data-autofocus], textarea, input, button:not([data-sheet-chrome])')?.focus()
    }, 40)
    return () => { window.clearTimeout(timer); previous?.focus?.() }
  }, [open])

  if (!open) return null

  function down(event: PointerEvent<HTMLDivElement>) {
    if ((event.target as Element).closest('button, select, input')) return
    drag.current = event.clientY
    sheet.current?.classList.add('is-dragging')
    event.currentTarget.setPointerCapture(event.pointerId)
  }
  function move(event: PointerEvent<HTMLDivElement>) {
    if (drag.current === null || !sheet.current) return
    sheet.current.style.transform = `translateY(${Math.max(0, event.clientY - drag.current)}px)`
  }
  function up(event: PointerEvent<HTMLDivElement>) {
    if (drag.current === null) return
    const distance = event.clientY - drag.current
    drag.current = null
    if (sheet.current) { sheet.current.classList.remove('is-dragging'); sheet.current.style.transform = '' }
    if (distance > 90) onClose()
    else if (distance < -60 && onSizeChange) onSizeChange('full')
  }

  return <>
    <button type="button" className="levi-sheet-scrim" aria-label="Close" tabIndex={-1} onClick={onClose}/>
    <section ref={sheet} id={id} role={role} aria-modal="true" className={`levi-sheet size-${size} ${className}`.trim()}>
      <div className="levi-sheet-handle" onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up}>
        <span className="levi-sheet-grip" aria-hidden="true"/>
        <div className="levi-sheet-title">
          <h2>{title}</h2>
          {headerExtra}
          {onSizeChange && <div className="levi-sheet-sizes" role="group" aria-label="Sheet height">
            <button type="button" data-sheet-chrome className={size === 'half' ? 'is-on' : ''} onClick={() => onSizeChange('half')}>Half</button>
            <button type="button" data-sheet-chrome className={size === 'full' ? 'is-on' : ''} onClick={() => onSizeChange('full')}>Full</button>
          </div>}
          <button type="button" data-sheet-chrome className="levi-sheet-close" onClick={onClose} aria-label="Close"><X size={16}/></button>
        </div>
      </div>
      <div className="levi-sheet-body">{children}</div>
      {footer && <div className="levi-sheet-footer">{footer}</div>}
    </section>
  </>
}
