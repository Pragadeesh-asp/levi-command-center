import { Check } from 'lucide-react'

export function SpringCheck({ checked, disabled, label, onChange }: { checked: boolean; disabled: boolean; label: string; onChange: (checked: boolean) => void }) {
  return <button type="button" role="checkbox" aria-checked={checked} aria-label={label} className={`spring-check${checked ? ' is-checked' : ''}`} disabled={disabled} onClick={() => onChange(!checked)}>
    <span className="spring-check-box">{checked && <Check size={17} strokeWidth={2.6}/>}</span>
  </button>
}
