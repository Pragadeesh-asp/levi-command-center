import { useState, type DragEvent } from 'react'
import { Timer } from 'lucide-react'

export interface BoardTask { id: string; title: string; status: string; priority: number; project_name: string | null; account_name: string | null; deadline: string | null; blocker: string }

/** The four columns, and the status a card gets when dropped into each. */
const COLUMNS = [
  { id: 'ready', title: 'Ready', status: 'ready', holds: ['backlog', 'ready'] },
  { id: 'in_progress', title: 'In progress', status: 'in_progress', holds: ['in_progress', 'waiting', 'review'] },
  { id: 'blocked', title: 'Blocked', status: 'blocked', holds: ['blocked'] },
  { id: 'done', title: 'Done', status: 'completed', holds: ['completed'] },
] as const

const label = (value: string) => value.replaceAll('_', ' ')

/**
 * Tasks as a board. Dragging a card (or choosing "Move to") saves through the same
 * save_task command as the editor, so the queue rules still apply and a refused move
 * shows the reason instead of moving the card.
 */
export function TaskBoard({ tasks, onMove, onOpen, onFocus }: { tasks: BoardTask[]; onMove: (task: BoardTask, status: string) => Promise<void>; onOpen: (id: string) => void; onFocus: (task: BoardTask) => void }) {
  const [dragging, setDragging] = useState<string | null>(null)
  const [over, setOver] = useState<string | null>(null)

  function drop(event: DragEvent<HTMLElement>, status: string) {
    event.preventDefault()
    setOver(null)
    const task = tasks.find(item => item.id === event.dataTransfer.getData('text/levi-task'))
    setDragging(null)
    if (task && task.status !== status) void onMove(task, status)
  }

  return <div className="task-board">
    {COLUMNS.map(column => {
      const cards = tasks.filter(task => (column.holds as readonly string[]).includes(task.status))
      return <section key={column.id} className={`board-column ${over === column.id ? 'is-over' : ''}`} aria-label={`${column.title}, ${cards.length} tasks`}
        onDragOver={event => { if (dragging) { event.preventDefault(); setOver(column.id) } }}
        onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node)) setOver(null) }}
        onDrop={event => drop(event, column.status)}>
        <header className="board-column-head"><h3>{column.title}</h3><span>{cards.length}</span></header>
        <div className="board-cards">
          {cards.length === 0 && <p className="board-empty">Drop a task here</p>}
          {cards.map(task => <article key={task.id} className={`board-card priority-${task.priority} ${dragging === task.id ? 'is-dragging' : ''}`} draggable
            onDragStart={event => { event.dataTransfer.setData('text/levi-task', task.id); event.dataTransfer.effectAllowed = 'move'; setDragging(task.id) }}
            onDragEnd={() => { setDragging(null); setOver(null) }}>
            <button type="button" className="board-card-title" onClick={() => onOpen(task.id)}>{task.title}</button>
            <div className="board-card-meta">
              <span>P{task.priority}</span>
              {task.project_name && <span>{task.project_name}</span>}
              {task.account_name && <span>{task.account_name}</span>}
              {task.deadline && <span>Due {task.deadline}</span>}
              {task.status !== column.status && column.id !== 'done' && <span className="board-substatus">{label(task.status)}</span>}
            </div>
            {task.status === 'blocked' && task.blocker && <p className="board-blocker">{task.blocker}</p>}
            <div className="board-card-actions">
              <label><span className="sr-only">Move {task.title} to</span>
                <select value="" onChange={event => { if (event.target.value) void onMove(task, event.target.value) }} aria-label={`Move ${task.title} to`}>
                  <option value="">Move to…</option>
                  {COLUMNS.filter(target => target.id !== column.id).map(target => <option key={target.id} value={target.status}>{target.title}</option>)}
                </select>
              </label>
              {column.id !== 'done' && <button type="button" className="board-focus" onClick={() => onFocus(task)} title="Focus on this task"><Timer size={13}/> Focus</button>}
            </div>
          </article>)}
        </div>
      </section>
    })}
  </div>
}
