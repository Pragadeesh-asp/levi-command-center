import { useEffect, useState } from 'react'
import { ArrowUpRight, Check, MicOff, Settings2, ShieldCheck, Volume2 } from 'lucide-react'

interface SettingsPageProps {
  voiceName: string
  speechRate: number
  onVoiceChange: (value: string) => void
  onSpeechRateChange: (value: number) => void
  onOpenAiSettings: () => void
}

export function SettingsPage({ voiceName, speechRate, onVoiceChange, onSpeechRateChange, onOpenAiSettings }: SettingsPageProps) {
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>([])
  const supported = 'speechSynthesis' in window

  useEffect(() => {
    if (!supported) return
    const update = () => setVoices(window.speechSynthesis.getVoices())
    update()
    window.speechSynthesis.addEventListener('voiceschanged', update)
    return () => window.speechSynthesis.removeEventListener('voiceschanged', update)
  }, [supported])

  return <div className="content settings-page">
    <header className="page-heading">
      <div><div className="eyebrow">APPLICATION CONTROL</div><h1>Settings</h1><p className="subtitle">Configure the capabilities LEVI currently provides.</p></div>
      <Settings2 size={24} aria-hidden="true" />
    </header>
    <div className="settings-grid">
      <section className="panel settings-card">
        <div className="settings-card-title"><span className="settings-icon"><Check size={17}/></span><div><h2>Appearance</h2><p>Current LEVI desktop palette</p></div></div>
        <div className="settings-palette"><span><i className="swatch-gunmetal"/>Gunmetal Black</span><span><i className="swatch-lunar"/>Lunar White</span><span><i className="swatch-lime"/>Neon Lime</span></div>
        <small>Theme switching is not currently available.</small>
      </section>

      <section className="panel settings-card">
        <div className="settings-card-title"><span className="settings-icon"><ShieldCheck size={17}/></span><div><h2>AI providers and models</h2><p>Provider credentials remain in the Rust-managed secure store.</p></div></div>
        <p>Configure providers, model selection, and connection checks in AI Command.</p>
        <button type="button" className="button-secondary settings-action" onClick={onOpenAiSettings}>Open AI Command <ArrowUpRight size={14}/></button>
      </section>

      <section className="panel settings-card">
        <div className="settings-card-title"><span className="settings-icon"><Volume2 size={17}/></span><div><h2>Voice and speech</h2><p>Local Windows speech playback. No audio is uploaded.</p></div></div>
        <label className="settings-control">Speech voice
          <select value={voiceName} onChange={event => onVoiceChange(event.target.value)} disabled={!supported}>
            <option value="">System default</option>
            {voices.map(voice => <option key={voice.voiceURI} value={voice.name}>{voice.name} · {voice.lang}</option>)}
          </select>
        </label>
        <label className="settings-control">Speech speed <span>{speechRate.toFixed(2)}×</span>
          <input type="range" min="0.75" max="1.25" step="0.05" value={speechRate} onChange={event => onSpeechRateChange(Number(event.target.value))} disabled={!supported}/>
        </label>
        <small>{supported ? 'Used when you choose Speak locally on an assistant response.' : 'Speech synthesis is not available in this WebView.'}</small>
      </section>

      <section className="panel settings-card">
        <div className="settings-card-title"><span className="settings-icon"><MicOff size={17}/></span><div><h2>Privacy and security</h2><p>Local-first controls currently in effect</p></div></div>
        <ul><li>Microphone input is not enabled.</li><li>AI requests are sent only after you submit them.</li><li>Consequential AI actions require review in AI Command.</li></ul>
      </section>

      <section className="panel settings-card settings-planned">
        <div className="settings-card-title"><span className="settings-icon"><Settings2 size={17}/></span><div><h2>Planned settings</h2><p>These controls are not implemented yet.</p></div></div>
        <p>Theme switching, application startup behavior, data export and restore, and notification preferences.</p>
      </section>
    </div>
  </div>
}
