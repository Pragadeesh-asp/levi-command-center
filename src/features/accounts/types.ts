export type BrowserFamily = 'chrome' | 'edge' | 'brave' | 'opera' | 'firefox'
export type AccountStatus = 'working' | 'waiting' | 'idle' | 'blocked' | 'completed' | 'needs_attention'

export interface AccountRecord {
  id: string
  name: string
  identifier: string | null
  browser_profile_id: string
  profile_name: string
  profile_path: string
  browser_id: string
  browser_name: string
  family: BrowserFamily
  conversation_url: string | null
  status: AccountStatus
  priority: number
  notes: string
  last_accessed_at: string | null
  project_id: string | null
  project_name: string | null
}

export interface ProfileOption {
  id: string
  browser_id: string
  browser_name: string
  family: BrowserFamily
  profile_name: string
  profile_path: string
  availability_status: string
}
