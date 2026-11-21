import { useCallback, useEffect, useRef, type ReactNode } from 'react'
import { VoiceBeam, type VoiceBeamMotion } from 'voice-glow'
import './voice-glow-input.css'

interface VoiceGlowInputProps {
  children: ReactNode
  processing?: boolean
  error?: boolean
  responseKey?: number
  className?: string
}

/** Shared chat-input treatment. Processing motion is driven by the package's
 * own animation loop through its public `motion` getter; no microphone is used. */
export function VoiceGlowInput({ children, processing = false, error = false, responseKey = 0, className = '' }: VoiceGlowInputProps) {
  const startedAt = useRef(0)
  const processingRef = useRef(processing)
  const reducedMotionRef = useRef(false)
  const glassRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const query = window.matchMedia('(prefers-reduced-motion: reduce)')
    const update = () => { reducedMotionRef.current = query.matches }
    update()
    query.addEventListener('change', update)
    return () => query.removeEventListener('change', update)
  }, [])

  useEffect(() => {
    processingRef.current = processing
    if (processing) startedAt.current = performance.now()
  }, [processing])

  useEffect(() => {
    if (!responseKey || reducedMotionRef.current) return
    const element = glassRef.current
    if (!element) return
    const animation = element.animate([
      { boxShadow: '0 0 0 0 rgba(22,70,160,0), inset 0 -1px rgba(220,38,56,.15)' },
      { boxShadow: '0 0 0 3px rgba(22,70,160,.18), inset 0 -2px rgba(125,206,245,.55)', offset: 0.35 },
      { boxShadow: '0 0 0 0 rgba(22,70,160,0), inset 0 -1px rgba(125,206,245,.14)' },
    ], { duration: 720, easing: 'ease-out' })
    return () => animation.cancel()
  }, [responseKey])

  const motion = useCallback((): VoiceBeamMotion | null => {
    if (!processingRef.current) return null
    if (reducedMotionRef.current) return { gather: 0.45, offset: 0, heldLevel: 0.38 }
    const elapsed = (performance.now() - startedAt.current) / 1000
    return {
      gather: 0.9,
      offset: Math.sin(elapsed * (Math.PI * 2 / 3.2)) * 0.72,
      stretch: 0.18,
      heldLevel: 0.5,
      cornerFollow: 0.25,
    }
  }, [])

  const state = error ? 'error' : processing ? 'processing' : 'idle'

  return <VoiceBeam
    type="default"
    theme="dark"
    colorVariant="colorful"
    strength={0.74}
    reach={0.82}
    spread={0.72}
    flow={18}
    idle={0.22}
    hueDuration={18}
    colors={['#7DCEF5', '#3977C9', '#DC2638', '#1646A0', '#7DCEF5', '#DC2638', '#3977C9']}
    motion={motion}
    active={!error}
    className={`levi-voice-beam ${className}`.trim()}
    data-voice-state={state}
    style={{ width: '100%', display: 'block' }}
  >
    <div ref={glassRef} className="levi-voice-glass">{children}</div>
  </VoiceBeam>
}
