import type { PointerEvent, ReactNode } from 'react'

/** LEVI-styled project card surface with a pointer spotlight; content remains usable without pointer input. */
export function MagicBentoCard({ children, className = '' }: { children: ReactNode; className?: string }) {
  function move(event: PointerEvent<HTMLElement>) {
    const bounds = event.currentTarget.getBoundingClientRect()
    event.currentTarget.style.setProperty('--spot-x', `${event.clientX - bounds.left}px`)
    event.currentTarget.style.setProperty('--spot-y', `${event.clientY - bounds.top}px`)
  }
  return <article className={`magic-bento-card ${className}`} onPointerMove={move} onPointerLeave={event => {
    event.currentTarget.style.removeProperty('--spot-x')
    event.currentTarget.style.removeProperty('--spot-y')
  }}>{children}</article>
}
