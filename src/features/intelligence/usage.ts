/** Provider-reported usage saved with a live reply (migration 010). */
export interface MessageUsage {
  promptTokens?: number | null
  completionTokens?: number | null
  latencyMs?: number | null
  toolMode?: string | null
}

/** "1,240 in · 312 out · 3.1 s", or '' when the provider reported nothing. */
export function usageLabel(usage: MessageUsage): string {
  const parts: string[] = []
  if (usage.promptTokens != null) parts.push(`${usage.promptTokens.toLocaleString()} in`)
  if (usage.completionTokens != null) parts.push(`${usage.completionTokens.toLocaleString()} out`)
  if (usage.latencyMs != null) parts.push(`${(usage.latencyMs / 1000).toFixed(1)} s`)
  if (parts.length && usage.toolMode === 'text') parts.push('text actions')
  return parts.join(' · ')
}

/** Marker JARVIS appends when a reply stopped at the provider's output-token limit. */
export const TRUNCATION_NOTE = 'Stopped at the output-token limit'
