import { useCallback, useEffect, useMemo, useState, type FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type Database from '@tauri-apps/plugin-sql'
import { AlertTriangle, FolderOpen, Globe2, HardDrive, Play, Plus, RefreshCw, Search, ShieldCheck, Trash2, X } from 'lucide-react'
import { errorMessage, newId } from '../../lib/database'
import type { BrowserFamily } from '../accounts/types'
import type { BrowserCandidate, BrowserRecord, LaunchFeedback, ProfileCandidate } from './types'
import './browsers.css'

interface RawBrowser extends Omit<BrowserRecord, 'profiles' | 'account_count'> {}
interface RawProfile extends ProfileCandidate { id: string; browser_id: string; lastAccessedAt: string | null; account_count: number }
interface BrowserForm { family: BrowserFamily; name: string; executable_path: string; browser_version: string | null; detection_method: string }
interface ProfileForm { profile_name: string; profile_path: string; user_data_directory: string }
const families: { id: BrowserFamily; title: string }[] = [{ id: 'chrome', title: 'Google Chrome' }, { id: 'edge', title: 'Microsoft Edge' }, { id: 'brave', title: 'Brave' }, { id: 'opera', title: 'Opera' }, { id: 'firefox', title: 'Firefox' }]
const familyLabel = (family: string) => families.find(v => v.id === family)?.title ?? family
const blankBrowser: BrowserForm = { family: 'chrome', name: '', executable_path: '', browser_version: null, detection_method: 'manual' }
const blankProfile: ProfileForm = { profile_name: '', profile_path: '', user_data_directory: '' }

interface Props { db: Database }
export function BrowserProfilesPage({ db }: Props) {
  const [browsers, setBrowsers] = useState<BrowserRecord[]>([])
  const [profiles, setProfiles] = useState<RawProfile[]>([])
  const [detected, setDetected] = useState<BrowserCandidate[]>([])
  const [profileCandidates, setProfileCandidates] = useState<ProfileCandidate[]>([])
  const [search, setSearch] = useState('')
  const [browserFormOpen, setBrowserFormOpen] = useState(false)
  const [profileBrowserId, setProfileBrowserId] = useState('')
  const [browserForm, setBrowserForm] = useState<BrowserForm>(blankBrowser)
  const [profileForm, setProfileForm] = useState<ProfileForm>(blankProfile)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)
  const [testUrl, setTestUrl] = useState('https://example.com/')
  const [launchingProfileId, setLaunchingProfileId] = useState('')
  const [awaitingManualClose, setAwaitingManualClose] = useState<{ id: string; name: string } | null>(null)

  const reload = useCallback(async () => {
    const [browserRows, profileRows] = await Promise.all([
      db.select<RawBrowser[]>(`SELECT b.id,b.name,b.family,b.executable_path,b.browser_version,b.detection_method,b.availability_status
        FROM browsers b ORDER BY b.name COLLATE NOCASE`),
      db.select<RawProfile[]>(`SELECT bp.id,bp.browser_id,bp.profile_name AS profileName,bp.profile_path AS profilePath,bp.user_data_directory AS userDataDirectory,bp.availability_status AS availabilityStatus,bp.detection_method AS detectionMethod,bp.last_accessed_at AS lastAccessedAt,
        (SELECT COUNT(*) FROM accounts a WHERE a.browser_profile_id=bp.id AND a.archived_at IS NULL) AS account_count
        FROM browser_profiles bp WHERE bp.archived_at IS NULL ORDER BY bp.profile_name COLLATE NOCASE`),
    ])
    const accountCounts = await db.select<{ browser_id: string; count: number }[]>(`SELECT bp.browser_id,COUNT(a.id) AS count FROM browser_profiles bp LEFT JOIN accounts a ON a.browser_profile_id=bp.id AND a.archived_at IS NULL GROUP BY bp.browser_id`)
    const countMap = new Map(accountCounts.map(v => [v.browser_id, v.count]))
    const byBrowser = new Map<string, RawProfile[]>()
    for (const profile of profileRows) byBrowser.set(profile.browser_id, [...(byBrowser.get(profile.browser_id) ?? []), profile])
    setProfiles(profileRows)
    setBrowsers(browserRows.map(row => ({ ...row, profiles: (byBrowser.get(row.id) ?? []).map(({ id: _id, browser_id: _browserId, lastAccessedAt: _last, account_count: _count, ...profile }) => profile), account_count: countMap.get(row.id) ?? 0 })))
  }, [db])

  // oxlint-disable-next-line react/set-state-in-effect -- Load registered browser and profile rows from SQLite.
  useEffect(() => { void reload().catch(e => setError(errorMessage(e))) }, [reload])
  const visibleBrowsers = useMemo(() => browsers.filter(b => `${b.name} ${familyLabel(b.family)} ${b.executable_path} ${b.profiles.map(p => p.profileName).join(' ')}`.toLowerCase().includes(search.toLowerCase())), [browsers, search])

  async function scanInstalled() {
    setError(''); setBusy(true)
    try { setDetected(await invoke<BrowserCandidate[]>('detect_browsers')); setNotice('Browser detection reads installation and profile display metadata only. It does not inspect credentials.'); }
    catch (e) { setError(errorMessage(e)) }
    finally { setBusy(false) }
  }

  async function pickExecutable() {
    const selected = await open({ title: 'Select browser executable', multiple: false, directory: false, filters: [{ name: 'Windows application', extensions: ['exe'] }] })
    if (typeof selected === 'string') setBrowserForm({ ...browserForm, executable_path: selected, detection_method: 'manual' })
  }

  async function addDetectedBrowser(candidate: BrowserCandidate) {
    setError(''); setBusy(true)
    try {
      const existing = await db.select<{ id: string }[]>('SELECT id FROM browsers WHERE lower(executable_path)=lower(?)', [candidate.executablePath])
      if (existing.length) { setError(`${candidate.name} is already registered. Use Rescan on its card.`); return }
      const validated = await invoke<{ canonicalPath: string; browserVersion: string | null }>('validate_browser_path', { family: candidate.family, executablePath: candidate.executablePath })
      const id = newId()
      await db.execute('INSERT INTO browsers(id,name,executable_path,family,browser_version,detection_method,availability_status) VALUES(?,?,?,?,?,?,?)', [id, candidate.name, validated.canonicalPath, candidate.family, validated.browserVersion, candidate.detectionMethod, 'available'])
      for (const profile of candidate.profiles.filter(p => p.availabilityStatus === 'available')) await upsertProfile(id, profile)
      setNotice(`${candidate.name} registered with ${candidate.profiles.length} discovered profile${candidate.profiles.length === 1 ? '' : 's'}.`)
      await reload()
    } catch (e) { setError(errorMessage(e)) }
    finally { setBusy(false) }
  }

  async function saveBrowser(event: FormEvent) {
    event.preventDefault(); setError(''); setBusy(true)
    try {
      const validated = await invoke<{ canonicalPath: string; browserVersion: string | null }>('validate_browser_path', { family: browserForm.family, executablePath: browserForm.executable_path })
      const duplicate = await db.select<{ id: string }[]>('SELECT id FROM browsers WHERE lower(executable_path)=lower(?)', [validated.canonicalPath])
      if (duplicate.length) throw new Error('That executable is already registered.')
      await db.execute('INSERT INTO browsers(id,name,executable_path,family,browser_version,detection_method,availability_status) VALUES(?,?,?,?,?,?,?)', [newId(), browserForm.name.trim() || familyLabel(browserForm.family), validated.canonicalPath, browserForm.family, validated.browserVersion, 'manual', 'available'])
      setBrowserFormOpen(false); setBrowserForm(blankBrowser); setNotice('Browser registered. Rescan it to find profiles.'); await reload()
    } catch (e) { setError(errorMessage(e)) }
    finally { setBusy(false) }
  }

  async function upsertProfile(browserId: string, candidate: ProfileCandidate) {
    await invoke<ProfileCandidate>('register_browser_profile', {
      browserId,
      profileName: candidate.profileName,
      profilePath: candidate.profilePath,
      userDataDirectory: candidate.userDataDirectory,
      detectionMethod: candidate.detectionMethod,
    })
  }

  async function rescan(browser: BrowserRecord) {
    setError(''); setBusy(true)
    try {
      const browserCheck = await invoke<{ canonicalPath: string; browserVersion: string | null }>('validate_browser_path', { family: browser.family, executablePath: browser.executable_path }).catch(() => null)
      if (!browserCheck) {
        await db.execute("UPDATE browsers SET availability_status='unavailable',updated_at=datetime('now') WHERE id=?", [browser.id])
        throw new Error(`${browser.name} could not be found at its registered executable path.`)
      }
      await db.execute("UPDATE browsers SET executable_path=?,browser_version=?,availability_status='available',updated_at=datetime('now') WHERE id=?", [browserCheck.canonicalPath, browserCheck.browserVersion, browser.id])
      const candidates = await invoke<ProfileCandidate[]>('discover_profiles', { browserId: browser.id })
      const refreshed = await invoke<{ id: string; availabilityStatus: string }[]>('refresh_profile_availability', { browserId: browser.id })
      for (const candidate of candidates) await upsertProfile(browser.id, candidate)
      for (const row of refreshed) await db.execute('UPDATE browser_profiles SET availability_status=? WHERE id=?', [row.availabilityStatus, row.id])
      await reload(); setNotice(`${browser.name}: found ${candidates.length} profile${candidates.length === 1 ? '' : 's'}.`)
    } catch (e) { setError(errorMessage(e)); await reload() }
    finally { setBusy(false) }
  }

  function addProfile(browserId: string, candidates: ProfileCandidate[]) {
    setProfileBrowserId(browserId); setProfileCandidates(candidates); setProfileForm({ ...blankProfile, user_data_directory: candidates[0]?.userDataDirectory ?? '' }); setError('')
  }

  async function saveProfile(event: FormEvent) {
    event.preventDefault(); setError(''); setBusy(true)
    const browser = browsers.find(b => b.id === profileBrowserId)
    if (!browser) { setBusy(false); setError('Select a registered browser first.'); return }
    try {
      await upsertProfile(browser.id, { profileName: profileForm.profile_name.trim(), profilePath: profileForm.profile_path.trim(), userDataDirectory: profileForm.user_data_directory.trim() || null, availabilityStatus: 'available', detectionMethod: 'manual' })
      setProfileBrowserId(''); setNotice('Profile registration saved. Browser files were not changed.'); await reload()
    } catch (e) { setError(errorMessage(e)) }
    finally { setBusy(false) }
  }

  async function pickFolder(field: 'profile_path' | 'user_data_directory') {
    const selected = await open({ title: field === 'profile_path' ? 'Select existing browser profile folder' : 'Select existing browser user-data folder', multiple: false, directory: true })
    if (typeof selected === 'string') {
      const next = { ...profileForm, [field]: selected }
      if (field === 'profile_path' && !next.profile_name) next.profile_name = selected.split(/[\\/]/).filter(Boolean).at(-1) ?? ''
      if (field === 'profile_path' && !next.user_data_directory) next.user_data_directory = selected.replace(/[\\/][^\\/]+$/, '')
      setProfileForm(next)
    }
  }

  async function removeRegistration(profile: RawProfile) {
    if (profile.account_count) { setError(`Reassign or archive the ${profile.account_count} linked account${profile.account_count === 1 ? '' : 's'} before removing this profile registration.`); return }
    if (!window.confirm(`Remove the registration for “${profile.profileName}”? The browser folder and its contents will not be deleted.`)) return
    try {
      await db.execute("UPDATE browser_profiles SET archived_at=datetime('now'),updated_at=datetime('now') WHERE id=?", [profile.id])
      setNotice('Profile registration removed. Browser profile data was left untouched.'); await reload()
    } catch (e) { setError(errorMessage(e)) }
  }

  async function testProfile(profile: RawProfile) {
    setError(''); setNotice(`Launch requested for ${profile.profileName}. Executable/profile validation and a process-conflict check are in progress.`); setLaunchingProfileId(profile.id)
    try { const feedback = await invoke<LaunchFeedback>('test_profile_launch', { profileId: profile.id, testUrl }); setNotice(feedback.message); setAwaitingManualClose({ id: profile.id, name: profile.profileName }); await reload() }
    catch (e) { setError(errorMessage(e)) }
    finally { setLaunchingProfileId('') }
  }

  return <div className="feature-page browser-page">
    <div className="feature-header"><div><div className="eyebrow">OPERATIONS / BROWSER PROFILES</div><h1>Browser profiles</h1><p>Register existing installations and profiles. LEVI never edits browser data.</p></div><div className="browser-header-actions"><button className="button-secondary" onClick={() => void scanInstalled()} disabled={busy}><Search size={14}/> Detect browsers</button><button className="button-primary" onClick={() => { setBrowserForm(blankBrowser); setError(''); setBrowserFormOpen(true) }}><Plus size={16}/> Add browser</button></div></div>
    {notice && <div className="inline-notice" role="status">{notice}</div>}{error && !browserFormOpen && !profileBrowserId && <div className="inline-error" role="alert">{error}</div>}
    <section className="browser-test-panel"><div><strong>Controlled test launch</strong><p>Add the installed browser manually, then register a newly created temporary profile. Do not use “Register” on detected installations for this test; it also registers discovered profile paths. Verify the browser window and selected page, then close it manually before the next test. Launch status remains unknown until you confirm what appeared.</p></div><label>Harmless test URL<select value={testUrl} onChange={e => setTestUrl(e.target.value)}><option value="https://example.com/">https://example.com/</option><option value="about:blank">about:blank (offline)</option></select></label></section>
    {awaitingManualClose && <div className="inline-notice manual-close-gate" role="status"><span>Manual step: close the browser window for test profile {awaitingManualClose.name}. The app cannot confirm the page loaded or that this process exited.</span><button className="button-secondary" onClick={() => setAwaitingManualClose(null)}>I closed it (or confirmed no window opened)</button></div>}
    {detected.length > 0 && <section className="detected-section"><div className="detected-heading"><div><h2>Detected installations</h2><p>Only executable and browser profile display metadata are read.</p></div><button className="card-icon-button" onClick={() => setDetected([])} aria-label="Dismiss detected browsers"><X size={15}/></button></div><div className="detected-list">{detected.map(candidate => <div className="detected-row" key={`${candidate.family}:${candidate.executablePath}`}><div className="browser-logo"><Globe2 size={17}/></div><div className="detected-meta"><strong>{candidate.name}</strong><span>{candidate.executablePath}</span><small>{candidate.profiles.length} profile{candidate.profiles.length === 1 ? '' : 's'} found · {candidate.detectionMethod}</small></div><button className="button-secondary" onClick={() => void addDetectedBrowser(candidate)} disabled={busy}><Plus size={13}/> Register</button></div>)}</div></section>}
    <label className="browser-search searchbox"><Search size={15}/><input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search browsers, paths, profiles"/></label>
    {visibleBrowsers.length ? <div className="browser-list">{visibleBrowsers.map(browser => {
      const browserProfiles = profiles.filter(p => p.browser_id === browser.id)
      return <section className="browser-card" key={browser.id}>
        <div className="browser-card-header"><div className="browser-logo"><Globe2 size={18}/></div><div className="browser-summary"><h2>{browser.name}</h2><p>{familyLabel(browser.family)} · {browser.browser_version ?? 'Unknown version'}</p></div><span className={`availability-tag ${browser.availability_status}`}>{browser.availability_status}</span><button className="button-secondary" disabled={busy} onClick={() => void rescan(browser)}><RefreshCw size={13}/> Rescan</button><button className="button-secondary" onClick={() => addProfile(browser.id, [])}><Plus size={13}/> Add profile</button></div>
        <div className="browser-path"><HardDrive size={13}/><span>{browser.executable_path}</span></div>
        <div className="browser-card-meta"><span><strong>{browserProfiles.length}</strong> registered profiles</span><span><strong>{browser.account_count}</strong> linked accounts</span><span>Added by {browser.detection_method === 'manual' ? 'you' : 'detection'}</span></div>
        {browserProfiles.length > 0 && <div className="profile-table"><div className="profile-table-head"><span>PROFILE</span><span>LOCATION</span><span>ACCOUNTS</span><span>AVAILABILITY</span><span>ACTIONS</span></div>{browserProfiles.map(profile => <div className="profile-row" key={profile.id}><div className="profile-name-cell"><FolderOpen size={14}/><strong>{profile.profileName}</strong></div><span className="profile-path-cell" title={profile.profilePath}>{profile.profilePath}</span><span className="profile-account-count">{profile.account_count}</span><span className={`availability-tag ${profile.availabilityStatus}`}>{profile.availabilityStatus}</span><div className="profile-actions"><button className="button-secondary test-launch-button" disabled={Boolean(launchingProfileId || awaitingManualClose) || profile.availabilityStatus !== 'available'} onClick={() => void testProfile(profile)} title="Test Launch" aria-label={`Test Launch ${profile.profileName}`}>{launchingProfileId === profile.id ? 'Starting…' : <><Play size={12}/> Test Launch</>}</button><button className="card-icon-button archive-action" disabled={awaitingManualClose?.id === profile.id} onClick={() => void removeRegistration(profile)} title="Remove registration" aria-label={`Remove ${profile.profileName} registration`}><Trash2 size={14}/></button></div></div>)}</div>}
        {!browserProfiles.length && <div className="no-profiles"><AlertTriangle size={14}/> No profiles registered yet. Rescan or add an existing profile folder.</div>}
      </section>
    })}</div> : <div className="empty-feature"><div className="empty-feature-icon"><Globe2 size={20}/></div><strong>{browsers.length ? 'No browsers match your search' : 'Register your existing browsers'}</strong><p>Detect installed browsers or add one manually, then register its existing profile folders.</p><button className="button-secondary" onClick={() => void scanInstalled()}><Search size={14}/> Detect installed browsers</button></div>}
    {browserFormOpen && <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) setBrowserFormOpen(false) }}><form className="form-modal" onSubmit={saveBrowser}>
      <div className="modal-heading"><div><span className="eyebrow">BROWSER REGISTRY</span><h2>Add browser installation</h2></div><button type="button" className="icon-button" onClick={() => setBrowserFormOpen(false)} aria-label="Close"><X size={17}/></button></div>
      <div className="modal-fields"><label>Browser family<select value={browserForm.family} onChange={e => setBrowserForm({ ...browserForm, family: e.target.value as BrowserFamily })}>{families.map(f => <option key={f.id} value={f.id}>{f.title}</option>)}</select></label><label>Display name<input maxLength={80} value={browserForm.name} onChange={e => setBrowserForm({ ...browserForm, name: e.target.value })} placeholder={familyLabel(browserForm.family)}/></label><label className="wide-field">Browser executable path<div className="path-picker"><input required value={browserForm.executable_path} onChange={e => setBrowserForm({ ...browserForm, executable_path: e.target.value })} placeholder="C:\Program Files\…\browser.exe"/><button type="button" className="button-secondary" onClick={() => void pickExecutable()}><FolderOpen size={13}/> Choose</button></div></label><div className="wide-field safety-note"><ShieldCheck size={14}/> Registration stores this path and does not change browser settings or profile data.</div></div>
      {error && <div className="inline-error" role="alert">{error}</div>}<div className="modal-actions"><button type="button" className="button-secondary" onClick={() => setBrowserFormOpen(false)}>Cancel</button><button className="button-primary" disabled={busy}>{busy ? 'Checking…' : 'Register browser'}</button></div>
    </form></div>}
    {!!profileBrowserId && <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) setProfileBrowserId('') }}><form className="form-modal" onSubmit={saveProfile}>
      <div className="modal-heading"><div><span className="eyebrow">PROFILE REGISTRY</span><h2>Add existing profile</h2></div><button type="button" className="icon-button" onClick={() => setProfileBrowserId('')} aria-label="Close"><X size={17}/></button></div>
      {profileCandidates.length > 0 && <label className="candidate-select">Detected profile<select defaultValue="" onChange={e => { const p = profileCandidates.find(c => c.profilePath === e.target.value); if (p) setProfileForm({ profile_name: p.profileName, profile_path: p.profilePath, user_data_directory: p.userDataDirectory ?? '' }) }}><option value="">Enter a different profile or select one</option>{profileCandidates.map(p => <option value={p.profilePath} key={p.profilePath}>{p.profileName} · {p.availabilityStatus}</option>)}</select></label>}
      <div className="modal-fields"><label>Profile display name<input required maxLength={100} value={profileForm.profile_name} onChange={e => setProfileForm({ ...profileForm, profile_name: e.target.value })} placeholder="e.g. Temporary test"/></label><label className="wide-field">Existing profile folder<div className="path-picker"><input required value={profileForm.profile_path} onChange={e => setProfileForm({ ...profileForm, profile_path: e.target.value })} placeholder="Select a newly created temporary profile directory"/><button type="button" className="button-secondary" onClick={() => void pickFolder('profile_path')}><FolderOpen size={13}/> Choose</button></div></label>{families.find(v => v.id === browsers.find(b => b.id === profileBrowserId)?.family)?.id !== 'firefox' && <label className="wide-field">Existing user-data directory<div className="path-picker"><input required value={profileForm.user_data_directory} onChange={e => setProfileForm({ ...profileForm, user_data_directory: e.target.value })} placeholder="Dedicated temporary parent directory"/><button type="button" className="button-secondary" onClick={() => void pickFolder('user_data_directory')}><FolderOpen size={13}/> Choose</button></div></label>}<div className="wide-field safety-note"><ShieldCheck size={14}/> For testing, create a new empty directory under TEMP and use only that directory. Never select a normal browser profile. For Chromium, create the profile folder directly inside its dedicated temporary user-data directory. Closing the test browser is manual; LEVI does not stop it or delete profile files.</div></div>
      {error && <div className="inline-error" role="alert">{error}</div>}<div className="modal-actions"><button type="button" className="button-secondary" onClick={() => setProfileBrowserId('')}>Cancel</button><button className="button-primary" disabled={busy}>{busy ? 'Validating…' : 'Register profile'}</button></div>
    </form></div>}
  </div>
}
