import { useRef, type KeyboardEvent, type ReactNode } from 'react'
import { ChevronDown, RefreshCw, Send, Square } from 'lucide-react'

export interface PromptModelOption { id: string; label: string }

interface PromptBarProps {
  id: string
  value: string
  placeholder: string
  disabled: boolean
  busy: boolean
  canSend: boolean
  model: string
  models?: PromptModelOption[]
  onModelChange?: (model: string) => void
  onLoadModels?: () => void
  loadingModels?: boolean
  onChange: (value: string) => void
  onStop?: () => void
  helper: string
  children: ReactNode
  simple?: boolean
}

/** Shared LEVI prompt surface; the enclosing form remains responsible for the real request operation. */
export function PromptBar({ id, value, placeholder, disabled, busy, canSend, model, models = [], onModelChange, onLoadModels, loadingModels = false, onChange, onStop, helper, children, simple = false }: PromptBarProps) {
  const input = useRef<HTMLTextAreaElement>(null)
  function resize() {
    const field = input.current
    if (!field) return
    field.style.height = 'auto'
    field.style.height = `${Math.min(field.scrollHeight, 176)}px`
  }
  function keyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      event.currentTarget.form?.requestSubmit()
    }
  }
  return <div className={`levi-promptbar${busy ? ' is-processing' : ''}`}>
    <textarea ref={input} id={id} value={value} onChange={event => { onChange(event.target.value); resize() }} onInput={resize} onKeyDown={keyDown} maxLength={12000} rows={1} placeholder={placeholder} aria-label="Message LEVI" disabled={disabled}/>
    <div className="levi-promptbar-footer">
      <div className="levi-promptbar-actions">{children}
        {!simple && (onModelChange ? <div className="promptbar-model">
          <select value={model} onChange={event => onModelChange(event.target.value)} disabled={disabled || models.length<2} aria-label="Model" title={models.length<2 ? 'Load the model list to choose another model' : 'Model for this message'}>{models.map(option => <option value={option.id} key={option.id}>{option.label}</option>)}</select>
          <ChevronDown size={12} aria-hidden="true"/>
          {onLoadModels&&<button type="button" className="promptbar-load-models" onClick={onLoadModels} disabled={disabled||loadingModels} aria-label="Load the model list" title="Load the model list from the provider"><RefreshCw size={13} className={loadingModels ? 'is-spinning' : ''}/></button>}
        </div> : <span className="promptbar-model-label" title="Change the model in AI Command settings">{model || 'No model set'}</span>)}
      </div>
      <div className="levi-promptbar-send">{busy && onStop ? <button type="button" className="button-primary promptbar-stop" onClick={onStop} aria-label="Stop generating"><Square size={13} fill="currentColor"/>Stop</button> : <button type="submit" className="button-primary" disabled={!canSend} aria-label="Send" title={helper}><Send size={14}/>Send</button>}</div>
    </div>
  </div>
}
