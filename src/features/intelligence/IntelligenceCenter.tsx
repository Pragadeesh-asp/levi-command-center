import { lazy, Suspense, useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type Database from '@tauri-apps/plugin-sql'
import {
  Bot, Check, ChevronDown, Clipboard, Mic, Plus, ShieldCheck,
  Sparkles, Square, Trash2, Volume2, Wifi,
} from 'lucide-react'
import { errorMessage } from '../../lib/database'
import { VoiceGlowInput } from './VoiceGlowInput'
import { ComposerActionMenu } from './ComposerActionMenu'
import { ThoughtLine, type ThoughtLineStatus } from './ThoughtLine'
import { useReducedMotion } from '../../lib/useReducedMotion'
import { PromptBar } from './PromptBar'
import { TRUNCATION_NOTE, usageLabel, type MessageUsage } from './usage'
import './intelligence.css'

type ProviderSlot='primary'|'secondary'
interface Config { slot:ProviderSlot;provider:string;baseUrl:string;model:string;hasApiKey:boolean;credentialStoreAvailable:boolean;credentialStatus:string;credentialStoreError:string|null;fallbackEnabled:boolean }
interface Conversation { id:string;title:string;createdAt:string;updatedAt:string }
interface Message extends MessageUsage { id:string;conversationId:string;role:string;responseKind:string;requestStatus:string|null;content:string;provider:string|null;model:string|null;createdAt:string }
interface TaskChoice { id:string;title:string;status:string;project_name:string|null }
interface ProjectChoice { id:string;name:string;status:string }
interface ActionProposal {
  id:string
  conversationId:string
  assistantMessageId:string
  actionKind:'create_task'|'create_reminder'
  argumentsJson:string
  contentHash:string
  status:'proposed'|'rejected'|'executed'
  lastError:string|null
  resultJson:string|null
  createdAt:string
  decidedAt:string|null
}
interface RequestRecovery { requestId:string;status:string;userMessage:string;leaseExpiresAt:string|null;recoveryAttempts:number }
interface RequestRecoverySummary { recovered:number;active:RequestRecovery[] }
interface ProviderModel { id:string;displayName:string|null }
interface RetryContext { text:string;outcomeUnknown:boolean;slot:ProviderSlot }
type ActionArguments =
  | {kind:'create_task';project_id:string;title:string;description:string;priority:number}
  | {kind:'create_reminder';title:string;body:string;due_at:string;project_id:string|null;task_id:string|null}
type AssistantState='IDLE'|'LISTENING'|'PROCESSING'|'THINKING'|'EXECUTING'|'SPEAKING'|'WAITING_FOR_APPROVAL'|'ERROR'
interface ThoughtTrace { status: ThoughtLineStatus; startedAt: number; finishedAt?: number; activity: string; assistantMessageId?: string }

const initialConfig:Config={slot:'primary',provider:'openai_compatible',baseUrl:'',model:'',hasApiKey:false,credentialStoreAvailable:false,credentialStatus:'checking',credentialStoreError:null,fallbackEnabled:false}
const initialSecondaryConfig:Config={...initialConfig,slot:'secondary',baseUrl:'https://openrouter.ai/api/v1'}
const label=(s:string)=>s.replaceAll('_',' ').replace(/\b\w/g,c=>c.toUpperCase())
const badgeText:Record<string,string>={live_ai:'LIVE AI RESPONSE',local_summary:'LOCAL SUMMARY',handoff_prompt:'LOCAL HANDOFF PROMPT',error:'REQUEST FAILED',outcome_unknown:'OUTCOME UNKNOWN · NOT RETRIED'}
const stateText:Record<AssistantState,string>={IDLE:'Ready',LISTENING:'Listening',PROCESSING:'Processing request',THINKING:'Waiting for provider response',EXECUTING:'Executing approved action',SPEAKING:'Speaking',WAITING_FOR_APPROVAL:'Action awaits your approval',ERROR:'Needs attention'}
const MarkdownMessage=lazy(()=>import('./MarkdownMessage'))
const providerSlotFrom=(provider:string|null):ProviderSlot=>provider?.endsWith('· secondary')?'secondary':'primary'
const providerProfileName=(slot:ProviderSlot)=>slot==='primary'?'Primary · FreeLLMAPI':'Secondary · OpenRouter'

export function IntelligenceCenter({db}:{db:Database}) {
  const [providerConfigs,setProviderConfigs]=useState<Record<ProviderSlot,Config>>({primary:initialConfig,secondary:initialSecondaryConfig})
  const [providerSlot,setProviderSlot]=useState<ProviderSlot>('primary')
  const config=providerConfigs[providerSlot]
  function setConfig(next:Config){setProviderConfigs(current=>({...current,[providerSlot]:next}))}
  const [apiKey,setApiKey]=useState('')
  const [conversations,setConversations]=useState<Conversation[]>([])
  const [messages,setMessages]=useState<Message[]>([])
  const [proposals,setProposals]=useState<ActionProposal[]>([])
  const [tasks,setTasks]=useState<TaskChoice[]>([])
  const [projects,setProjects]=useState<ProjectChoice[]>([])
  const [conversationId,setConversationId]=useState('')
  const [draft,setDraft]=useState('')
  const [handoffTask,setHandoffTask]=useState('')
  const [handoffKind,setHandoffKind]=useState('chatgpt_review')
  const [loading,setLoading]=useState(false)
  const [busy,setBusy]=useState(false)
  const [responseSequence,setResponseSequence]=useState(0)
  const [testing,setTesting]=useState(false)
  const [discoveringModels,setDiscoveringModels]=useState(false)
  const [providerModels,setProviderModels]=useState<ProviderModel[]>([])
  const [connectionChecks,setConnectionChecks]=useState<Record<ProviderSlot,boolean|null>>({primary:null,secondary:null})
  const [activeRequestId,setActiveRequestId]=useState<string|null>(null)
  const activeRequestRef=useRef<string|null>(null)
  const cancelRequestedRef=useRef(false)
  const messagesEndRef=useRef<HTMLDivElement|null>(null)
  const [assistantState,setAssistantState]=useState<AssistantState>('IDLE')
  const [error,setError]=useState('')
  const [notice,setNotice]=useState('')
  const [activeRequests,setActiveRequests]=useState<RequestRecovery[]>([])
  const [retryContext,setRetryContext]=useState<RetryContext|null>(null)
  const [thought,setThought]=useState<ThoughtTrace|null>(null)
  const reducedMotion=useReducedMotion()

  const reloadConversations=useCallback(async()=>{
    const [configRow,secondaryRow,rows,taskRows,projectRows]=await Promise.all([
      invoke<Config>('get_ai_config'),
      invoke<Config>('get_ai_secondary_config'),
      invoke<Conversation[]>('list_ai_conversations'),
      db.select<TaskChoice[]>("SELECT t.id,t.title,t.status,p.name AS project_name FROM tasks t LEFT JOIN projects p ON p.id=t.project_id WHERE t.archived_at IS NULL ORDER BY t.updated_at DESC LIMIT 200"),
      db.select<ProjectChoice[]>("SELECT id,name,status FROM projects WHERE archived_at IS NULL ORDER BY name COLLATE NOCASE"),
    ])
    setProviderConfigs({primary:configRow,secondary:secondaryRow})
    setConversations(rows)
    setTasks(taskRows)
    setProjects(projectRows)
    setHandoffTask(current=>taskRows.some(t=>t.id===current)?current:taskRows[0]?.id??'')
    setConversationId(current=>rows.some(c=>c.id===current)?current:rows[0]?.id??'')
  },[db])

  // oxlint-disable-next-line react/set-state-in-effect -- Load persisted provider settings, conversations and task choices on mount.
  useEffect(()=>{void reloadConversations().catch(e=>setError(errorMessage(e)))},[reloadConversations])

  const reloadMessages=useCallback(async()=>{
    if(!conversationId){setMessages([]);setProposals([]);setAssistantState('IDLE');return}
    setLoading(true)
    try{
      const [rows,actionRows,recovery]=await Promise.all([
        invoke<Message[]>('get_ai_messages',{conversationId}),
        invoke<ActionProposal[]>('get_ai_action_proposals',{conversationId}),
        invoke<RequestRecoverySummary>('recover_interrupted_ai_requests'),
      ])
      setMessages(rows)
      setProposals(actionRows)
      setActiveRequests(recovery.active)
      const last=rows.at(-1)
      if(last?.role==='assistant'&&last.responseKind==='error'){
        const priorUser=[...rows.slice(0,-1)].reverse().find(m=>m.role==='user')
        if(priorUser)setRetryContext({text:priorUser.content,outcomeUnknown:last.requestStatus==='outcome_unknown',slot:providerSlotFrom(last.provider)})
      }else setRetryContext(null)
      if(!busy){setAssistantState(actionRows.some(a=>a.status==='proposed')?'WAITING_FOR_APPROVAL':'IDLE')}
    }catch(e){setError(errorMessage(e))}finally{setLoading(false)}
  },[conversationId,busy])

  // oxlint-disable-next-line react/set-state-in-effect -- Refresh the selected persisted conversation when selection changes.
  useEffect(()=>{void reloadMessages()},[reloadMessages])

  useEffect(()=>{
    const node=messagesEndRef.current
    if(node)node.scrollIntoView({behavior:window.matchMedia('(prefers-reduced-motion: reduce)').matches?'auto':'smooth',block:'end'})
  },[messages,loading])

  async function createConversation(){setError('');setNotice('');try{const row=await invoke<Conversation>('create_ai_conversation');await reloadConversations();setConversationId(row.id)}catch(e){setError(errorMessage(e))}}
  async function saveConfig(e:FormEvent){e.preventDefault();setBusy(true);setError('');setNotice('');try{const saved=await invoke<Config>('save_ai_config',{input:{...config,slot:providerSlot,apiKey:apiKey||null}});setConfig(saved);setApiKey('');setProviderModels([]);setConnectionChecks(current=>({...current,[providerSlot]:null}));setNotice(saved.hasApiKey?'Provider settings saved. The API key is held in Windows Credential Manager.':'Provider settings saved; add an API key before connecting.')}catch(e){setError(errorMessage(e))}finally{setBusy(false)}}
  async function selectPromptModel(model:string){
    if(!model||model===config.model||!providerModels.some(item=>item.id===model))return
    setBusy(true);setError('');setNotice('')
    try{const saved=await invoke<Config>('save_ai_config',{input:{slot:providerSlot,provider:config.provider,baseUrl:config.baseUrl,model,apiKey:null}});setProviderConfigs(current=>({...current,[providerSlot]:saved}));setNotice(`Selected ${model} for the next ${providerProfileName(providerSlot)} request.`)}catch(e){setError(errorMessage(e))}finally{setBusy(false)}
  }
  async function removeKey(){setBusy(true);setError('');setNotice('');try{await invoke('remove_ai_api_key',{slot:providerSlot});setConfig({...config,hasApiKey:false,credentialStatus:'credential_missing'});setNotice('Provider API key removed from Windows Credential Manager.')}catch(e){setError(errorMessage(e))}finally{setBusy(false)}}
  async function discoverModels(){
    if(!config.hasApiKey||!config.baseUrl)return
    const confirmed=window.confirm(`LEVI will send an authenticated model-list request to ${config.baseUrl}. This sends the provider key to that configured endpoint, but no prompt or workspace data. Continue?`)
    if(!confirmed)return
    setDiscoveringModels(true);setError('');setNotice('')
    try{
      const models=await invoke<ProviderModel[]>('list_ai_models',{slot:providerSlot})
      setProviderModels(models)
      setNotice(`Loaded ${models.length} model identifiers from the configured endpoint. Availability can still depend on provider access and quota.`)
    }catch(e){setError(errorMessage(e))}finally{setDiscoveringModels(false)}
  }
  async function testConnection(){
    if(!window.confirm('LEVI will send one short test prompt to the configured provider. This may incur a small provider charge. Continue?'))return
    setTesting(true);setConnectionChecks(current=>({...current,[providerSlot]:null}));setError('');setNotice('')
    try{const result=await invoke<{connected:boolean;message:string;provider:string;model:string}>('test_ai_connection',{slot:providerSlot});setConnectionChecks(current=>({...current,[providerSlot]:true}));setNotice(`${result.message} Profile: ${providerSlot}; protocol: ${result.provider}; model: ${result.model}.`)}
    catch(e){setConnectionChecks(current=>({...current,[providerSlot]:false}));setError(errorMessage(e))}finally{setTesting(false)}
  }
  async function submitProviderMessage(sentText:string,requestSlot:ProviderSlot=providerSlot):Promise<boolean>{
    if(!conversationId||!sentText.trim()||busy)return false
    const requestId=crypto.randomUUID()
    const startedAt=Date.now()
    cancelRequestedRef.current=false
    activeRequestRef.current=requestId
    setThought({status:'working',startedAt,activity:'Processing your question'})
    setActiveRequestId(requestId);setBusy(true);setAssistantState('PROCESSING');setError('');setNotice('')
    try{
      const response=await invoke<Message>('send_ai_message',{input:{conversationId,message:sentText,requestId,providerSlot:requestSlot}})
      await reloadMessages()
      await reloadConversations()
      if(response.responseKind==='error'){
        setRetryContext({text:sentText,outcomeUnknown:response.requestStatus==='outcome_unknown',slot:requestSlot})
        setAssistantState('ERROR')
        const status:ThoughtLineStatus=cancelRequestedRef.current?'cancelled':response.content.includes('stopped at its output-token limit')?'incomplete':response.requestStatus==='outcome_unknown'?'unknown':'error'
        setThought({status,startedAt,finishedAt:Date.now(),activity:response.content,assistantMessageId:response.id})
        return false
      }else{
        const truncated=response.content.includes(TRUNCATION_NOTE)
        setDraft('');setConnectionChecks(current=>({...current,[requestSlot]:true}));setResponseSequence(value=>value+1)
        setThought(truncated
          ?{status:'incomplete',startedAt,finishedAt:Date.now(),activity:'The reply stopped at the output-token limit. LEVI saved the partial text; ask JARVIS to continue.',assistantMessageId:response.id}
          :{status:'complete',startedAt,finishedAt:Date.now(),activity:'The provider returned a complete response and LEVI saved it.',assistantMessageId:response.id})
      }
      return true
    }catch(e){setError(errorMessage(e));await reloadMessages();setAssistantState('ERROR');setThought({status:'unknown',startedAt,finishedAt:Date.now(),activity:'The request command failed before LEVI could confirm its final provider outcome.'});return false}
    finally{cancelRequestedRef.current=false;activeRequestRef.current=null;setActiveRequestId(null);setBusy(false)}
  }
  async function sendMessage(e:FormEvent){
    e.preventDefault()
    await submitProviderMessage(draft)
  }
  async function cancelActiveRequest(){
    const requestId=activeRequestId
    if(!requestId)return
    setNotice('Stop requested. LEVI is waiting for the provider request to settle safely.')
    try{await invoke('cancel_ai_request',{requestId});cancelRequestedRef.current=true}
    catch(e){setError(errorMessage(e))}
  }
  async function retryProviderRequest(targetSlot:ProviderSlot=retryContext?.slot??providerSlot){
    if(!conversationId||!retryContext||busy)return
    if(targetSlot!==retryContext.slot){
      const target=providerConfigs[targetSlot]
      if(!config.fallbackEnabled||!target.hasApiKey||!target.model||!target.baseUrl)return
      const uncertain=retryContext.outcomeUnknown?' The earlier request may already have been processed and charged, so this can cause a duplicate charge.':''
      if(!window.confirm(`Send a new request through ${providerProfileName(targetSlot)} using a new request ID?${uncertain} LEVI will not automatically switch providers.`))return
    }else if(retryContext.outcomeUnknown&&!window.confirm('The previous provider request may already have been processed and charged. LEVI will create a new request ID; this may result in a duplicate charge. Send again?'))return
    const sentText=retryContext.text
    setRetryContext(null)
    if(await submitProviderMessage(sentText,targetSlot))setNotice(`The new request through ${providerProfileName(targetSlot)} completed and was saved. Review it before continuing.`)
  }
  async function toggleManualFallback(enabled:boolean){
    try{await invoke<boolean>('set_ai_fallback_enabled',{enabled});setProviderConfigs(current=>({primary:{...current.primary,fallbackEnabled:enabled},secondary:{...current.secondary,fallbackEnabled:enabled}}));setNotice(enabled?'Manual fallback is enabled. LEVI will only use the other provider after you choose it in a recovery prompt.':'Manual fallback is disabled.')}catch(e){setError(errorMessage(e))}
  }
  async function localSummary(){if(!conversationId)return;setBusy(true);setAssistantState('PROCESSING');setError('');try{await invoke('add_local_workforce_summary',{conversationId});await reloadMessages();await reloadConversations()}catch(e){setError(errorMessage(e));setAssistantState('ERROR')}finally{setBusy(false)}}
  async function makeHandoff(){if(!conversationId||!handoffTask)return;setBusy(true);setAssistantState('PROCESSING');setError('');try{await invoke('add_handoff_prompt',{input:{conversationId,taskId:handoffTask,kind:handoffKind}});await reloadMessages();await reloadConversations()}catch(e){setError(errorMessage(e));setAssistantState('ERROR')}finally{setBusy(false)}}
  async function resolveAction(proposal:ActionProposal,approve:boolean){
    if(approve&&!window.confirm(`Approve this exact action?\n\n${proposalSummary(proposal,projects)}\n\nLEVI will create this record in the local workspace.`))return
    setBusy(true);setAssistantState(approve?'EXECUTING':'PROCESSING');setError('');setNotice('')
    try{
      const updated=await invoke<ActionProposal>('resolve_ai_action_proposal',{proposalId:proposal.id,expectedHash:proposal.contentHash,approve})
      setNotice(updated.status==='executed'?'Approved action completed and saved.':updated.status==='rejected'?'Action proposal rejected.':`Action status: ${label(updated.status)}.`)
      await reloadMessages();await reloadConversations()
      if(updated.status!=='executed'&&updated.status!=='rejected')setAssistantState('ERROR')
    }catch(e){setError(errorMessage(e));await reloadMessages();setAssistantState('ERROR')}
    finally{setBusy(false)}
  }
  async function copy(text:string){try{await navigator.clipboard.writeText(text);setNotice('Copied. The package has not been sent externally.')}catch{setError('Clipboard access is unavailable. Select and copy the text manually.')}}

  const connectionChecked=connectionChecks[providerSlot]
  const providerConnectionLabel=connectionChecked===true?'CONNECTED THIS SESSION':connectionChecked===false?'CONNECTION FAILED':config.hasApiKey?'CREDENTIAL SAVED · NOT VERIFIED':config.credentialStatus==='checking'?'CHECKING SECURE STORE':'SETUP REQUIRED'
  const providerConnectionClass=connectionChecked===true?'stored':connectionChecked===false?'failed':'missing'
  function draftComposerPrompt(prompt:string){setDraft(prompt);requestAnimationFrame(()=>document.getElementById('ai-composer-input')?.focus())}
  function focusHandoff(){document.getElementById('ai-handoff-task')?.focus()}

  return <div className="feature-page intelligence-page">
    <div className="feature-header ai-page-heading"><div><div className="eyebrow">JARVIS / COMMAND INTELLIGENCE</div><h1>LEVI Intelligence</h1><p>Live provider responses, local summaries and proposed actions remain clearly separated.</p></div><span className="manual-badge"><ShieldCheck size={14}/> User-controlled</span></div>
    {notice&&<div className="inline-notice" role="status">{notice}</div>}{error&&<div className="inline-error" role="alert">{error}</div>}
    {activeRequests.length>0&&<div className="inline-notice" role="status"><strong>Provider request still processing.</strong> LEVI will not send a duplicate. {activeRequests.map(r=><span key={r.requestId}> Check again after {r.leaseExpiresAt??'the recovery lease expires'}.</span>)}</div>}
    <details className="panel ai-settings" open={!config.model||!config.hasApiKey}><summary><span><ChevronDown size={15}/><strong>Provider connection</strong><small>{connectionChecked===true?'A request succeeded in this session':config.hasApiKey?'Credential saved securely; connectivity not verified':'Not configured'}</small></span><span className={`ai-connection ${providerConnectionClass}`}><i/>{providerConnectionLabel}</span></summary>
      <div className="provider-profile-tabs" role="group" aria-label="Provider profile">
        <button type="button" className={providerSlot==='primary'?'selected':''} onClick={()=>{setProviderSlot('primary');setApiKey('');setProviderModels([])}}>Primary · FreeLLMAPI</button>
        <button type="button" className={providerSlot==='secondary'?'selected':''} onClick={()=>{setProviderSlot('secondary');setApiKey('');setProviderModels([])}}>Secondary · OpenRouter</button>
      </div>
      <div className={`credential-status credential-${config.credentialStatus}`} role="status"><ShieldCheck size={13}/><span>{config.credentialStatus==='checking'?'Checking Windows Credential Manager…':config.credentialStatus==='store_unavailable'?`Windows Credential Manager is unavailable (${config.credentialStoreError??'backend_error'}). Key saving is disabled; LEVI will not use a plaintext fallback.`:config.credentialStatus==='credential_lookup_failed'?`Windows Credential Manager is reachable, but lookup failed (${config.credentialStoreError??'lookup_error'}).`:config.credentialStatus==='credential_available'?'A credential is saved in Windows Credential Manager for this provider endpoint.':config.credentialStatus==='credential_missing'?'Windows Credential Manager is ready; no credential is saved for this endpoint.':config.credentialStatus==='configuration_invalid'?`Provider configuration is invalid (${config.credentialStoreError??'invalid_configuration'}). No credential lookup was attempted.`:config.credentialStatus==='configuration_changed'?'Provider configuration changed. Save settings to check the credential for this exact endpoint.':'Provider endpoint is not configured; no credential lookup was attempted.'}</span></div>
      <form className="ai-settings-grid" onSubmit={saveConfig}>
        <label>Provider protocol<select value={config.provider} onChange={e=>{setConfig({...config,provider:e.target.value,model:'',hasApiKey:false,credentialStatus:'configuration_changed',baseUrl:e.target.value==='anthropic'?'https://api.anthropic.com':providerSlot==='secondary'?'https://openrouter.ai/api/v1':''});setApiKey('');setProviderModels([]);setConnectionChecks(current=>({...current,[providerSlot]:null}))}}><option value="openai_compatible">OpenAI-compatible API</option><option value="anthropic">Anthropic Messages API</option></select><small>{providerSlot==='primary'?"FreeLLMAPI is the primary profile. Its exact deployment base URL remains unset until you provide it.":"OpenRouter's documented OpenAI-compatible API is selected for the secondary profile."}</small></label>
        <label>Base URL<input required maxLength={300} value={config.baseUrl} onChange={e=>{setConfig({...config,baseUrl:e.target.value,hasApiKey:false,credentialStatus:e.target.value.trim()?'configuration_changed':'configuration_missing'});setApiKey('');setConnectionChecks(current=>({...current,[providerSlot]:null}))}} placeholder={providerSlot==='secondary'?'https://openrouter.ai/api/v1':config.provider==='anthropic'?'https://api.anthropic.com':'Enter the provider-supplied HTTPS base URL'}/></label>
        <label>Model identifier<input list="levi-provider-models" maxLength={120} value={config.model} onChange={e=>setConfig({...config,model:e.target.value})} placeholder="Load available models or enter an exact ID"/><datalist id="levi-provider-models">{providerModels.map(model=><option key={model.id} value={model.id}>{model.displayName??model.id}</option>)}</datalist><small>Model discovery is optional; compatible model IDs can also be entered manually.</small></label>
        <label>API key<input type="password" autoComplete="new-password" maxLength={2048} value={apiKey} onChange={e=>setApiKey(e.target.value)} placeholder={config.hasApiKey?'Saved securely; blank keeps current key':'Enter provider API key'} disabled={!config.credentialStoreAvailable}/><small>Sent directly to the Rust backend and stored in Windows Credential Manager. Never saved in SQLite or frontend storage.</small></label>
        <div className="ai-settings-actions"><button className="button-primary" disabled={busy||!config.credentialStoreAvailable}>{busy?'Saving…':'Save provider settings'}</button><button type="button" className="button-secondary" disabled={!config.hasApiKey||discoveringModels||busy} onClick={()=>void discoverModels()}><ChevronDown size={14}/>{discoveringModels?'Loading models…':'Load supported models'}</button><button type="button" className="button-secondary" disabled={!config.model||!config.hasApiKey||testing||busy} onClick={()=>void testConnection()}><Wifi size={14}/>{testing?'Testing…':'Test connection'}</button>{config.hasApiKey&&<button type="button" className="button-secondary danger-text" disabled={busy} onClick={()=>void removeKey()}><Trash2 size={13}/> Remove API key</button>}</div>
      </form>
      <label className="manual-fallback-setting"><input type="checkbox" checked={config.fallbackEnabled} onChange={e=>void toggleManualFallback(e.target.checked)}/><span><strong>Allow manual provider fallback</strong><small>LEVI never switches during a request. After an error, you may choose the other configured profile; it always sends a new request ID. Unknown outcomes require duplicate-charge confirmation.</small></span></label>
    </details>
    <div className="ai-data-disclosure"><ShieldCheck size={15}/><span>When you send a chat message, LEVI sends that message, recent turns, and a curated snapshot of saved projects, tasks, accounts, reminders, deliverables and recent activity to the configured provider. The API key is excluded from prompt/context data and is sent only as the authentication header to the exact configured endpoint. Do not include secrets. The endpoint operator and its upstream providers may process the request under their own data and billing policies.</span></div>

    <div className={`ai-assistant-state state-${assistantState.toLowerCase()}`} role="status" aria-live="polite">
      <div className="ai-state-copy"><span className="eyebrow">LEVI / COMMAND INTELLIGENCE</span><strong>{assistantState}</strong><small>{stateText[assistantState]}</small></div>
      <div className="ai-voice-controls">{activeRequestId&&<button type="button" className="button-secondary stop-generation" onClick={()=>void cancelActiveRequest()}><Square size={13} fill="currentColor"/>Stop generation</button>}<button type="button" className="button-secondary" disabled title="Local speech recognition is not configured. No microphone permission is requested and no audio is sent."><Mic size={14}/>Push to talk unavailable</button><button type="button" className="button-secondary" disabled title="Speech synthesis is not configured in this build."><Volume2 size={14}/>Speak unavailable</button></div>
    </div>
    <div className="ai-context-chip"><ShieldCheck size={14}/><span>Workspace context is selected by Rust from saved LEVI records. Conversation text is sent only when you submit a provider request.</span></div>

    <div className="ai-layout">
      <aside className="panel ai-conversation-list"><div className="ai-conversation-heading"><div><span className="eyebrow">LOCAL HISTORY</span><strong>Conversations</strong></div><button className="button-primary" onClick={()=>void createConversation()}><Plus size={14}/> New</button></div>{conversations.length?conversations.map(c=><button key={c.id} className={`ai-conversation-row ${conversationId===c.id?'selected':''}`} onClick={()=>setConversationId(c.id)}><strong>{c.title}</strong><small>{c.updatedAt}</small></button>):<div className="handoff-empty"><strong>No conversations</strong><span>Create a conversation to store AI responses and local summaries.</span></div>}</aside>
      <main className="panel ai-chat-panel">
        <div className="ai-chat-toolbar"><div><Bot size={16}/><strong>Command Intelligence</strong><span>{config.model||'Provider not configured'}</span><select aria-label="Active provider profile" value={providerSlot} onChange={e=>{setProviderSlot(e.target.value as ProviderSlot);setApiKey('');setProviderModels([])}}><option value="primary">Primary · FreeLLMAPI</option><option value="secondary">Secondary · OpenRouter</option></select></div><button className="button-secondary" disabled={!conversationId||busy} onClick={()=>void localSummary()}><Sparkles size={13}/> Local workforce summary</button></div>
        <div className="ai-suggestions"><span className="eyebrow">ASK ABOUT SAVED WORKSPACE DATA</span>{['Summarize active projects','Which tasks are blocked and why?','What reminders are coming up?'].map(text=><button type="button" key={text} className="ai-suggestion" onClick={()=>setDraft(text)}>{text}</button>)}</div>
        <div className="ai-message-list">{!conversationId?<div className="handoff-empty"><strong>Start a conversation</strong><span>Your history stays in the local SQLite workspace.</span></div>:loading?<div className="handoff-empty">Loading conversation history…</div>:messages.length?messages.map(m=><article key={m.id} className={`ai-message ${m.role}`}><div className="ai-message-meta"><span className={`ai-message-badge ${m.requestStatus==='outcome_unknown'?'outcome_unknown':m.responseKind}`}>{m.requestStatus==='outcome_unknown'?badgeText.outcome_unknown:badgeText[m.responseKind]??label(m.responseKind)}</span>{m.provider&&<small>{m.provider} · {m.model}</small>}{usageLabel(m)&&<small className="ai-message-usage" title="Reported by the provider">{usageLabel(m)}</small>}<time>{m.createdAt}</time></div>{thought?.assistantMessageId===m.id&&<ThoughtLine status={thought.status} startedAt={thought.startedAt} finishedAt={thought.finishedAt} activity={thought.activity} reducedMotion={reducedMotion}/>}<Suspense fallback={<div className="ai-markdown-loading">Rendering message…</div>}><MarkdownMessage content={m.content}/></Suspense>
          {proposals.filter(p=>p.assistantMessageId===m.id).map(proposal=><ActionProposalCard key={proposal.id} proposal={proposal} projects={projects} busy={busy} onResolve={resolveAction}/>)}
          {m.role==='assistant'&&<button className="text-button" onClick={()=>void copy(m.content)}><Clipboard size={13}/> Copy</button>}</article>):<div className="handoff-empty"><strong>Ask about your real workspace</strong><span>Try a suggestion, ask a question, or request a task/reminder proposal.</span></div>}{busy&&thought?.status==='working'&&<ThoughtLine status={thought.status} startedAt={thought.startedAt} activity={thought.activity} reducedMotion={reducedMotion}/>}<div ref={messagesEndRef}/></div>
        {retryContext&&<div className="inline-notice request-recovery" role="alert"><strong>{retryContext.outcomeUnknown?'Outcome unknown. No duplicate was sent.':'Provider rejected the request.'}</strong><span>{retryContext.outcomeUnknown?'Review provider usage before retrying. Any retry uses a new request ID and may cause a duplicate charge.':'A retry sends a new request after you choose its provider.'}</span><button type="button" className="button-secondary" disabled={busy||!providerConfigs[retryContext.slot].hasApiKey||!providerConfigs[retryContext.slot].model} onClick={()=>void retryProviderRequest(retryContext.slot)}>{retryContext.outcomeUnknown?'Send again with original provider…':'Retry with original provider'}</button>{config.fallbackEnabled&&<button type="button" className="button-secondary" disabled={busy||!providerConfigs[retryContext.slot==='primary'?'secondary':'primary'].hasApiKey||!providerConfigs[retryContext.slot==='primary'?'secondary':'primary'].model} onClick={()=>void retryProviderRequest(retryContext.slot==='primary'?'secondary':'primary')}>Send new request via {providerProfileName(retryContext.slot==='primary'?'secondary':'primary')}</button>}</div>}
        <form className="ai-composer" onSubmit={sendMessage}><VoiceGlowInput processing={busy} error={assistantState==='ERROR'} responseKey={responseSequence} className="ai-composer-beam"><PromptBar id="ai-composer-input" value={draft} onChange={value=>{setDraft(value);if(assistantState==='ERROR')setAssistantState('IDLE')}} placeholder="Ask about projects, tasks, workers, queue blockers or handoffs…" disabled={!conversationId||busy} busy={busy} canSend={Boolean(conversationId&&draft.trim()&&config.hasApiKey&&config.model&&!retryContext)} model={config.model} models={[...providerModels.map(item=>({id:item.id,label:item.displayName??item.id})),...(config.model&&!providerModels.some(item=>item.id===config.model)?[{id:config.model,label:`${config.model} · configured` }]:[])]} onModelChange={selectPromptModel} onLoadModels={()=>void discoverModels()} loadingModels={discoveringModels} onStop={()=>void cancelActiveRequest()} helper={conversationId?'Enter to send · Shift+Enter for a new line.':'Create a conversation before sending.'}><ComposerActionMenu onLocalSummary={()=>void localSummary()} onDraftPrompt={draftComposerPrompt} onPrepareHandoff={focusHandoff}/></PromptBar></VoiceGlowInput></form>
      </main>
    </div>
    <section className="panel ai-handoff-tool"><div><span className="eyebrow">PROMPT GENERATOR</span><h2>Prepare a task handoff</h2><p>Builds a local prompt from saved task requirements and the latest handoff package. It is not sent to an external service.</p></div><div className="ai-handoff-controls"><select id="ai-handoff-task" aria-label="Task" value={handoffTask} onChange={e=>setHandoffTask(e.target.value)}><option value="">Select task</option>{tasks.map(t=><option key={t.id} value={t.id}>{t.title} · {t.project_name??'No project'} · {label(t.status)}</option>)}</select><select aria-label="Handoff type" value={handoffKind} onChange={e=>setHandoffKind(e.target.value)}><option value="chatgpt_review">ChatGPT review</option><option value="claude_revision">Claude revision</option><option value="copilot_implementation">Copilot implementation</option><option value="verification">Verification</option></select><button className="button-secondary" disabled={!conversationId||!handoffTask||busy} onClick={()=>void makeHandoff()}><Check size={14}/> Generate locally</button></div></section>
    <div className="ai-capability-note"><ShieldCheck size={15}/><span>AI may propose task or one-time reminder creation only. Proposal contents are validated again on approval and execute through existing Rust transactions. Assignment, archive, browser launch, SQL and shell actions remain unavailable.</span></div>
  </div>
}

function proposalSummary(proposal:ActionProposal,projects:ProjectChoice[]):string {
  try {
    const args=JSON.parse(proposal.argumentsJson) as ActionArguments
    if(args.kind==='create_task'){
      const project=projects.find(p=>p.id===args.project_id)
      return `Create task “${args.title}” in ${project?.name??`project ${args.project_id}`} (priority ${args.priority}). Description: ${args.description||'None provided.'}`
    }
    const project=args.project_id?projects.find(p=>p.id===args.project_id)?.name??`Project ${args.project_id}`:'No project link'
    return `Create one-time reminder “${args.title}” for ${args.due_at}. ${args.body||'No details.'} Linked to ${project}${args.task_id?`; task ${args.task_id}`:''}.`
  } catch { return 'This saved proposal could not be displayed safely.' }
}

function ActionProposalCard({proposal,projects,busy,onResolve}:{proposal:ActionProposal;projects:ProjectChoice[];busy:boolean;onResolve:(proposal:ActionProposal,approve:boolean)=>void}) {
  const pending=proposal.status==='proposed'
  const args:ActionArguments|null=(()=>{try{return JSON.parse(proposal.argumentsJson) as ActionArguments}catch{return null}})()
  return <section className={`ai-action-proposal proposal-${proposal.status}`} aria-label="AI action proposal">
    <div className="ai-proposal-heading"><ShieldCheck size={15}/><strong>{pending?'Approval required':label(proposal.status)}</strong><span>{label(proposal.actionKind)}</span></div>
    <p>{proposalSummary(proposal,projects)}</p>
    {args&&<dl>{args.kind==='create_task'?<><dt>Task</dt><dd>{args.title}</dd><dt>Project</dt><dd>{projects.find(p=>p.id===args.project_id)?.name??args.project_id}</dd><dt>Priority</dt><dd>{args.priority}</dd><dt>Description</dt><dd>{args.description||'None'}</dd></>:<><dt>Reminder</dt><dd>{args.title}</dd><dt>Due</dt><dd>{args.due_at}</dd><dt>Details</dt><dd>{args.body||'None'}</dd></>}</dl>}
    {proposal.lastError&&<div className="inline-error" role="alert">{proposal.lastError}</div>}
    {proposal.resultJson&&<small className="ai-action-result">Saved result: {proposal.resultJson}</small>}
    {pending&&<div className="ai-proposal-actions"><button type="button" className="button-primary" disabled={busy||!args} onClick={()=>onResolve(proposal,true)}><Check size={14}/> Approve and create</button><button type="button" className="button-secondary" disabled={busy} onClick={()=>onResolve(proposal,false)}>Reject</button></div>}
  </section>
}
