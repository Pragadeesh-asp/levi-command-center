import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type Database from '@tauri-apps/plugin-sql'
import { Bot, Check, ChevronDown, Clipboard, Plus, Send, ShieldCheck, Sparkles, Trash2, Wifi } from 'lucide-react'
import { errorMessage } from '../../lib/database'
import './intelligence.css'

interface Config { provider:string;baseUrl:string;model:string;hasApiKey:boolean;credentialStoreAvailable:boolean;credentialStoreError:string|null }
interface Conversation { id:string;title:string;createdAt:string;updatedAt:string }
interface Message { id:string;conversationId:string;role:string;responseKind:string;content:string;provider:string|null;model:string|null;createdAt:string }
interface TaskChoice { id:string;title:string;status:string;project_name:string|null }
const initialConfig:Config={provider:'openai_compatible',baseUrl:'https://api.openai.com/v1',model:'',hasApiKey:false,credentialStoreAvailable:false,credentialStoreError:null}
const label=(s:string)=>s.replaceAll('_',' ').replace(/\b\w/g,c=>c.toUpperCase())
const badgeText:Record<string,string>={live_ai:'LIVE AI RESPONSE',local_summary:'LOCAL SUMMARY',handoff_prompt:'LOCAL HANDOFF PROMPT',error:'REQUEST ERROR'}

export function IntelligenceCenter({db}:{db:Database}) {
  const [config,setConfig]=useState(initialConfig)
  const [apiKey,setApiKey]=useState('')
  const [conversations,setConversations]=useState<Conversation[]>([])
  const [messages,setMessages]=useState<Message[]>([])
  const [tasks,setTasks]=useState<TaskChoice[]>([])
  const [conversationId,setConversationId]=useState('')
  const [draft,setDraft]=useState('')
  const [handoffTask,setHandoffTask]=useState('')
  const [handoffKind,setHandoffKind]=useState('chatgpt_review')
  const [loading,setLoading]=useState(false)
  const [busy,setBusy]=useState(false)
  const [testing,setTesting]=useState(false)
  const [error,setError]=useState('')
  const [notice,setNotice]=useState('')

  const reloadConversations=useCallback(async()=>{
    const [configRow,rows,taskRows]=await Promise.all([
      invoke<Config>('get_ai_config'),
      invoke<Conversation[]>('list_ai_conversations'),
      db.select<TaskChoice[]>("SELECT t.id,t.title,t.status,p.name AS project_name FROM tasks t LEFT JOIN projects p ON p.id=t.project_id WHERE t.archived_at IS NULL ORDER BY t.updated_at DESC LIMIT 200"),
    ])
    setConfig(configRow);setConversations(rows);setTasks(taskRows);setHandoffTask(current=>taskRows.some(t=>t.id===current)?current:taskRows[0]?.id??'')
    setConversationId(current=>rows.some(c=>c.id===current)?current:rows[0]?.id??'')
  },[db])
  // oxlint-disable-next-line react/set-state-in-effect -- Load persisted provider settings, conversations and task choices on mount.
  useEffect(()=>{void reloadConversations().catch(e=>setError(errorMessage(e)))},[reloadConversations])
  const reloadMessages=useCallback(async()=>{
    if(!conversationId){setMessages([]);return}
    setLoading(true)
    try{setMessages(await invoke<Message[]>('get_ai_messages',{conversationId}))}catch(e){setError(errorMessage(e))}finally{setLoading(false)}
  },[conversationId])
  // oxlint-disable-next-line react/set-state-in-effect -- Refresh the selected persisted conversation when selection changes.
  useEffect(()=>{void reloadMessages()},[reloadMessages])

  async function createConversation(){setError('');setNotice('');try{const row=await invoke<Conversation>('create_ai_conversation');await reloadConversations();setConversationId(row.id)}catch(e){setError(errorMessage(e))}}
  async function saveConfig(e:FormEvent){e.preventDefault();setBusy(true);setError('');setNotice('');try{const saved=await invoke<Config>('save_ai_config',{input:{...config,apiKey:apiKey||null}});setConfig(saved);setApiKey('');setNotice(saved.hasApiKey?'Provider settings saved. The API key is held in Windows Credential Manager.':'Provider settings saved; add an API key before connecting.')}catch(e){setError(errorMessage(e))}finally{setBusy(false)}}
  async function removeKey(){setBusy(true);setError('');setNotice('');try{await invoke('remove_ai_api_key');setConfig({...config,hasApiKey:false});setNotice('Provider API key removed from Windows Credential Manager.')}catch(e){setError(errorMessage(e))}finally{setBusy(false)}}
  async function testConnection(){if(!window.confirm('LEVI will send one short test prompt to the configured provider. This may incur a small API charge. Continue?'))return;setTesting(true);setError('');setNotice('');try{const result=await invoke<{connected:boolean;message:string;provider:string;model:string}>('test_ai_connection');setNotice(`${result.message} Provider: ${result.provider}; model: ${result.model}.`)}catch(e){setError(errorMessage(e))}finally{setTesting(false)}}
  async function sendMessage(e:FormEvent){e.preventDefault();if(!conversationId||!draft.trim())return;setBusy(true);setError('');setNotice('');try{await invoke('send_ai_message',{input:{conversationId,message:draft}});setDraft('');await reloadMessages();await reloadConversations()}catch(e){setError(errorMessage(e));await reloadMessages()}finally{setBusy(false)}}
  async function localSummary(){if(!conversationId)return;setBusy(true);setError('');try{await invoke('add_local_workforce_summary',{conversationId});await reloadMessages();await reloadConversations()}catch(e){setError(errorMessage(e))}finally{setBusy(false)}}
  async function makeHandoff(){if(!conversationId||!handoffTask)return;setBusy(true);setError('');try{await invoke('add_handoff_prompt',{input:{conversationId,taskId:handoffTask,kind:handoffKind}});await reloadMessages();await reloadConversations()}catch(e){setError(errorMessage(e))}finally{setBusy(false)}}
  async function copy(text:string){try{await navigator.clipboard.writeText(text);setNotice('Copied. The package has not been sent externally.')}catch{setError('Clipboard access is unavailable. Select and copy the text manually.')}}

  return <div className="feature-page intelligence-page">
    <div className="feature-header"><div><div className="eyebrow">INTELLIGENCE / CONTROLLED ASSISTANCE</div><h1>AI Command Intelligence</h1><p>Live provider responses are labeled separately from local database summaries. LEVI cannot execute actions or arbitrary commands.</p></div><span className="manual-badge"><ShieldCheck size={14}/> User-controlled</span></div>
    {notice&&<div className="inline-notice" role="status">{notice}</div>}{error&&<div className="inline-error" role="alert">{error}</div>}
    <details className="panel ai-settings" open={!config.model||!config.hasApiKey}><summary><span><ChevronDown size={15}/><strong>Provider connection</strong><small>{config.hasApiKey?'Credential saved securely':'Not connected'}</small></span><span className={`ai-connection ${config.hasApiKey?'stored':'missing'}`}><i/>{config.hasApiKey?'KEY STORED':'SETUP REQUIRED'}</span></summary>
      {!config.credentialStoreAvailable&&<div className="inline-error ai-keyring-error">Windows Credential Manager is unavailable{config.credentialStoreError?`: ${config.credentialStoreError}`:''}. LEVI will not store an API key in a fallback file or database.</div>}
      <form className="ai-settings-grid" onSubmit={saveConfig}>
        <label>Provider type<select value={config.provider} onChange={e=>setConfig({...config,provider:e.target.value,hasApiKey:false,baseUrl:e.target.value==='anthropic'?'https://api.anthropic.com':'https://api.openai.com/v1'})}><option value="openai_compatible">OpenAI-compatible API</option><option value="anthropic">Anthropic Claude API</option></select></label>
        <label>Base URL<input required maxLength={300} value={config.baseUrl} onChange={e=>setConfig({...config,baseUrl:e.target.value,hasApiKey:false})} placeholder="https://api.openai.com/v1"/></label>
        <label>Model identifier<input required maxLength={120} value={config.model} onChange={e=>setConfig({...config,model:e.target.value})} placeholder="Enter a model supported by this provider"/></label>
        <label>API key<input type="password" autoComplete="new-password" maxLength={2048} value={apiKey} onChange={e=>setApiKey(e.target.value)} placeholder={config.hasApiKey?'Saved securely; blank keeps current key':'Enter provider API key'} disabled={!config.credentialStoreAvailable}/><small>Sent directly to the Rust backend and stored in Windows Credential Manager. Never saved in SQLite or frontend storage.</small></label>
        <div className="ai-settings-actions"><button className="button-primary" disabled={busy||!config.credentialStoreAvailable}>{busy?'Saving…':'Save provider settings'}</button><button type="button" className="button-secondary" disabled={!config.model||!config.hasApiKey||testing||busy} onClick={()=>void testConnection()}><Wifi size={14}/>{testing?'Testing…':'Test connection'}</button>{config.hasApiKey&&<button type="button" className="button-secondary danger-text" disabled={busy} onClick={()=>void removeKey()}><Trash2 size={13}/> Remove API key</button>}</div>
      </form>
    </details>
    <div className="ai-data-disclosure"><ShieldCheck size={15}/><span>When you send a chat message, LEVI sends that message, recent turns and a curated snapshot of saved projects, tasks, accounts, reminders and deliverables to the configured provider. Do not include secrets. The provider's own data and billing policies apply.</span></div>
    <div className="ai-layout">
      <aside className="panel ai-conversation-list"><div className="ai-conversation-heading"><div><span className="eyebrow">LOCAL HISTORY</span><strong>Conversations</strong></div><button className="button-primary" onClick={()=>void createConversation()}><Plus size={14}/> New</button></div>{conversations.length?conversations.map(c=><button key={c.id} className={`ai-conversation-row ${conversationId===c.id?'selected':''}`} onClick={()=>setConversationId(c.id)}><strong>{c.title}</strong><small>{c.updatedAt}</small></button>):<div className="handoff-empty"><strong>No conversations</strong><span>Create a conversation to store AI responses and local summaries.</span></div>}</aside>
      <main className="panel ai-chat-panel"><div className="ai-chat-toolbar"><div><Bot size={16}/><strong>Command Intelligence</strong><span>{config.model||'Provider not configured'}</span></div><button className="button-secondary" disabled={!conversationId||busy} onClick={()=>void localSummary()}><Sparkles size={13}/> Local workforce summary</button></div>
        <div className="ai-message-list">{!conversationId?<div className="handoff-empty"><strong>Start a conversation</strong><span>Your history stays in the local SQLite workspace.</span></div>:loading?<div className="handoff-empty">Loading conversation history…</div>:messages.length?messages.map(m=><article key={m.id} className={`ai-message ${m.role}`}><div className="ai-message-meta"><span className={`ai-message-badge ${m.responseKind}`}>{badgeText[m.responseKind]??label(m.responseKind)}</span>{m.provider&&<small>{m.provider} · {m.model}</small>}<time>{m.createdAt}</time></div><pre>{m.content}</pre>{m.role==='assistant'&&<button className="text-button" onClick={()=>void copy(m.content)}><Clipboard size={13}/> Copy</button>}</article>):<div className="handoff-empty"><strong>Ask about your real workspace</strong><span>Try: “Which ready tasks are blocked and why?” or add a local workforce summary.</span></div>}</div>
        <form className="ai-composer" onSubmit={sendMessage}><textarea maxLength={12000} rows={3} value={draft} onChange={e=>setDraft(e.target.value)} placeholder="Ask about projects, tasks, workers, queue blockers or handoffs…" disabled={!conversationId||busy}/><div><small>{conversationId?'Live responses use saved workforce context. No actions are executed.':'Create a conversation before sending.'}</small><button className="button-primary" disabled={!conversationId||!draft.trim()||busy||!config.hasApiKey||!config.model}><Send size={14}/>{busy?'Working…':'Send to provider'}</button></div></form>
      </main>
    </div>
    <section className="panel ai-handoff-tool"><div><span className="eyebrow">PROMPT GENERATOR</span><h2>Prepare a task handoff</h2><p>Builds a local prompt from saved task requirements and the latest handoff package. It is not sent to an external service.</p></div><div className="ai-handoff-controls"><select aria-label="Task" value={handoffTask} onChange={e=>setHandoffTask(e.target.value)}><option value="">Select task</option>{tasks.map(t=><option key={t.id} value={t.id}>{t.title} · {t.project_name??'No project'} · {label(t.status)}</option>)}</select><select aria-label="Handoff type" value={handoffKind} onChange={e=>setHandoffKind(e.target.value)}><option value="chatgpt_review">ChatGPT review</option><option value="claude_revision">Claude revision</option><option value="copilot_implementation">Copilot implementation</option><option value="verification">Verification</option></select><button className="button-secondary" disabled={!conversationId||!handoffTask||busy} onClick={()=>void makeHandoff()}><Check size={14}/> Generate locally</button></div></section>
    <div className="ai-capability-note"><ShieldCheck size={15}/><span>Action recommendations are advisory. Task assignment, reminder changes and completion must be performed in their dedicated screens and confirmed there. LEVI has no SQL tool, shell access, browser automation, or external-agent execution.</span></div>
  </div>
}
