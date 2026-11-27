import { useLayoutEffect, useState, type RefObject } from 'react'
import { flushSync } from 'react-dom'

const reduced = () => window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false

/**
 * Runs a navigation inside a view transition, so the old screen cross-fades into the new
 * one. When `from` is given (an Overview tile), that element morphs into the new screen's
 * first heading. Falls back to an instant change where unsupported or with reduced motion.
 */
export function navigateWithTransition(update: () => void, from?: HTMLElement | null) {
  const doc = document as Document & { startViewTransition?: (cb: () => void) => { finished: Promise<void> } }
  if (!doc.startViewTransition || reduced()) { update(); return }
  if (from) from.style.viewTransitionName = 'levi-hero'
  const transition = doc.startViewTransition(() => {
    if (from) from.style.viewTransitionName = ''
    flushSync(update)
    const heading = document.querySelector<HTMLElement>('.workspace-scroll h1')
    if (from && heading) heading.style.viewTransitionName = 'levi-hero'
  })
  void transition.finished.finally(() => {
    document.querySelectorAll<HTMLElement>('.workspace-scroll h1').forEach(heading => { heading.style.viewTransitionName = '' })
  })
}

/**
 * Positions a sliding indicator under the active item of a tab row. Returns the style for
 * an absolutely positioned element; the row needs `position: relative`.
 */
export function useIndicator(row: RefObject<HTMLElement | null>, activeKey: string | number) {
  const [style, setStyle] = useState<{ left: number; width: number; ready: boolean }>({ left: 0, width: 0, ready: false })
  useLayoutEffect(() => {
    const measure = () => {
      const active = row.current?.querySelector<HTMLElement>('[aria-selected="true"], [aria-current="page"]')
      if (!active) return
      // oxlint-disable-next-line react/set-state-in-effect -- measuring layout to place the indicator
      setStyle(previous => ({ left: active.offsetLeft, width: active.offsetWidth, ready: previous.ready || previous.width > 0 }))
    }
    measure()
    const observer = new ResizeObserver(measure)
    if (row.current) observer.observe(row.current)
    return () => observer.disconnect()
  }, [row, activeKey])
  return { transform: `translateX(${style.left}px)`, width: style.width, transition: style.ready ? undefined : 'none', opacity: style.width ? 1 : 0 }
}

