import { ListTodo, Sparkles, Workflow } from 'lucide-react'

interface ComposerActionMenuProps {
  onLocalSummary?: () => void
  onDraftPrompt: (prompt: string) => void
  onPrepareHandoff?: () => void
}

/** Quick actions as plain chips beside the prompt: always visible, one click each. */
export function ComposerActionMenu({ onLocalSummary, onDraftPrompt, onPrepareHandoff }: ComposerActionMenuProps) {
  const actions = [
    ...(onLocalSummary ? [{ label: 'Summary', title: 'Add a local workforce summary', icon: Sparkles, run: onLocalSummary }] : []),
    { label: 'Blockers', title: 'Ask which tasks are blocked and why', icon: ListTodo, run: () => onDraftPrompt('Which tasks are blocked, and what dependencies or file claims block them?') },
    ...(onPrepareHandoff ? [{ label: 'Handoff', title: 'Prepare a task handoff', icon: Workflow, run: onPrepareHandoff }] : []),
  ]
  return <div className="composer-chips" role="group" aria-label="Quick actions">
    {actions.map(({ label, title, icon: Icon, run }) => <button key={label} type="button" className="composer-chip" title={title} onClick={run}><Icon size={13}/>{label}</button>)}
  </div>
}
