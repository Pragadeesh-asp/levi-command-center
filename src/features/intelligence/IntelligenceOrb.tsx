import { ThinkingOrb, type OrbState } from 'thinking-orbs'

interface IntelligenceOrbProps {
  state: OrbState
  size: 20 | 64
  paused?: boolean
  label: string
}

export function IntelligenceOrb({ state, size, paused = false, label }: IntelligenceOrbProps) {
  return <ThinkingOrb state={state} size={size} theme="dark" paused={paused} aria-label={label} className="levi-thinking-orb" />
}
