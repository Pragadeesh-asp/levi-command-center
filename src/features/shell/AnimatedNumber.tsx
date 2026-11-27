import { useEffect, useRef, useState } from 'react'

const reduced = () => window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false

/** A number that rolls to its new value when it changes, and is still otherwise. */
export function AnimatedNumber({ value }: { value: number | null | undefined }) {
  const [shown, setShown] = useState(value ?? null)
  const previous = useRef(value ?? null)
  useEffect(() => {
    const from = previous.current
    previous.current = value ?? null
    if (value == null || from == null || from === value || reduced()) { setShown(value ?? null); return }
    const start = performance.now()
    let frame = 0
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / 450)
      const eased = 1 - (1 - t) ** 3
      setShown(Math.round(from + (value - from) * eased))
      if (t < 1) frame = requestAnimationFrame(step)
    }
    frame = requestAnimationFrame(step)
    return () => cancelAnimationFrame(frame)
  }, [value])
  return <>{shown ?? '—'}</>
}
