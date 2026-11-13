import type { BrowserFamily } from '../accounts/types'

export interface ProfileCandidate {
  profileName: string
  profilePath: string
  userDataDirectory: string | null
  availabilityStatus: 'available' | 'unavailable'
  detectionMethod: 'automatic' | 'manual' | string
}

export interface BrowserCandidate {
  family: BrowserFamily
  name: string
  executablePath: string
  browserVersion: string | null
  detectionMethod: string
  profiles: ProfileCandidate[]
}

export interface BrowserRecord {
  id: string
  name: string
  family: BrowserFamily
  executable_path: string
  browser_version: string | null
  detection_method: string
  availability_status: string
  profiles: ProfileCandidate[]
  account_count: number
}
