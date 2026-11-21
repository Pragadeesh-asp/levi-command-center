import { invoke } from '@tauri-apps/api/core'

/** A LEVI Browser Space (profile), as published in its ecosystem manifest. */
export interface Space { id: string; name: string; avatar: string; accent: string }

export interface BrowserStatus {
  registered: boolean
  launchable: boolean
  version: string | null
  updatedAtUnix: number | null
  spaces: Space[]
  problem: string | null
}

export interface EcosystemStatus { browser: BrowserStatus; leviOsUrl: string | null; leviOsSpace: string | null }
export interface OpenInSpaceResult { processId: number; spaceName: string; host: string }

export const getEcosystem = () => invoke<EcosystemStatus>('get_ecosystem')

export const saveLeviOs = (url: string, spaceId: string) =>
  invoke<EcosystemStatus>('save_levi_os', { url: url.trim() || null, spaceId: spaceId || null })

export const openInSpace = (spaceId: string, url: string, accountId?: string) =>
  invoke<OpenInSpaceResult>('open_in_levi_browser', { spaceId, url, accountId: accountId ?? null })

export const openLeviBrowser = () => invoke<number>('open_levi_browser')

/** Opens LEVI OS in the Space chosen in Settings. */
export function openLeviOs(status: EcosystemStatus) {
  if (!status.leviOsUrl) return Promise.reject(new Error('Add your LEVI OS address in Settings › LEVI ecosystem first.'))
  if (!status.leviOsSpace) return Promise.reject(new Error('Choose the LEVI Browser Space that opens LEVI OS in Settings first.'))
  return openInSpace(status.leviOsSpace, status.leviOsUrl)
}

export function hostOf(url: string | null) {
  if (!url) return ''
  try { return new URL(url).host } catch { return url }
}

export function goToSettings() {
  window.dispatchEvent(new CustomEvent('navigate', { detail: 'Settings' }))
}
