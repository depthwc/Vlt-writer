import { create } from 'zustand'

import i18n, { systemLang, type Lang } from '@/i18n'
import { api, inTauri } from '@/lib/api'

export type Workspace = 'write' | 'hands' | 'papers' | 'settings'
/** white or black */
export type Theme = 'light' | 'dark'

/** A theme kept by an earlier version (blackboard, high contrast, match Windows) as one of the two. */
function themeOf(v: string | null): Theme {
  if (v === 'dark' || v === 'board') return 'dark'
  if (v === 'system') return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  return 'light'
}

interface AppState {
  ws: Workspace
  theme: Theme
  lang: Lang
  scale: number
  reducedMotion: boolean
  projectId: string | null
  handId: string | null
  paperId: string | null
  paletteOpen: boolean
  tourOpen: boolean
  exportOpen: boolean
  /** characters a document needs that were never written, to be written next */
  /** characters a document needs that are not written yet (in print letters or joined) */
  toWrite: { chars: string; print: boolean } | null
  setToWrite: (s: { chars: string; print: boolean } | null) => void
  /** the name of the document open in Write (for the title bar) */
  docName: string | null
  setDocName: (s: string | null) => void
  setWs: (w: Workspace) => void
  setTheme: (t: Theme) => void
  setLang: (l: Lang) => void
  setScale: (s: number) => void
  setReducedMotion: (v: boolean) => void
  setProjectId: (id: string | null) => void
  setHandId: (id: string | null) => void
  setPaperId: (id: string | null) => void
  setPaletteOpen: (v: boolean) => void
  setTourOpen: (v: boolean) => void
  setExportOpen: (v: boolean) => void
}

export function persist(key: string, value: string) {
  try {
    localStorage.setItem(`hw.${key}`, value)
  } catch {
    /* private mode: fine */
  }
  if (inTauri()) void api.setSetting(key, value).catch(() => {})
}

export function local(key: string): string | null {
  try {
    return localStorage.getItem(`hw.${key}`)
  } catch {
    return null
  }
}

export function applyTheme(t: Theme) {
  document.documentElement.dataset.theme = t
}

export const useApp = create<AppState>((set) => ({
  ws: (local('ws') as Workspace) || 'write',
  theme: themeOf(local('theme')),
  lang: (local('lang') as Lang) || systemLang(),
  scale: Number(local('scale') || 1),
  reducedMotion: local('reducedMotion') === '1',
  projectId: local('projectId'),
  handId: local('handId'),
  paperId: local('paperId'),
  paletteOpen: false,
  tourOpen: false,
  exportOpen: false,
  toWrite: null,
  setToWrite: (toWrite) => set({ toWrite }),
  docName: null,
  setDocName: (docName) => set({ docName }),
  setWs: (ws) => {
    persist('ws', ws)
    set({ ws })
  },
  setTheme: (theme) => {
    persist('theme', theme)
    applyTheme(theme)
    set({ theme })
  },
  setLang: (lang) => {
    persist('lang', lang)
    void i18n.changeLanguage(lang)
    document.documentElement.lang = lang
    set({ lang })
  },
  setScale: (scale) => {
    persist('scale', String(scale))
    document.documentElement.style.setProperty('--ui-scale', String(scale))
    set({ scale })
  },
  setReducedMotion: (reducedMotion) => {
    persist('reducedMotion', reducedMotion ? '1' : '0')
    document.documentElement.dataset.motion = reducedMotion ? 'reduce' : ''
    set({ reducedMotion })
  },
  setProjectId: (projectId) => {
    persist('projectId', projectId ?? '')
    set({ projectId })
  },
  setHandId: (handId) => {
    persist('handId', handId ?? '')
    set({ handId })
  },
  setPaperId: (paperId) => {
    persist('paperId', paperId ?? '')
    set({ paperId })
  },
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  setTourOpen: (tourOpen) => set({ tourOpen }),
  setExportOpen: (exportOpen) => set({ exportOpen }),
}))

/** Apply persisted look before first paint; then pull settings saved in the library. */
/** Choices kept in the library too: brought back when the interface's own storage lost them. */
export async function hydrateSettings() {
  if (!inTauri()) return
  try {
    for (const [k, v] of await api.allSettings()) {
      if (local(k) === null) localStorage.setItem(`hw.${k}`, v)
    }
  } catch {
    /* no storage: the defaults stand */
  }
}

export async function bootSettings() {
  const s = useApp.getState()
  applyTheme(s.theme)
  document.documentElement.style.setProperty('--ui-scale', String(s.scale))
  document.documentElement.dataset.motion = s.reducedMotion ? 'reduce' : ''
  document.documentElement.lang = s.lang
  void i18n.changeLanguage(s.lang)
  if (!inTauri()) return
  const [lang, theme, scale, tour] = await Promise.all(['lang', 'theme', 'scale', 'tourDone'].map((k) => api.getSetting(k).catch(() => null)))
  if (lang && lang !== s.lang) s.setLang(lang as Lang)
  if (theme && themeOf(theme) !== s.theme) s.setTheme(themeOf(theme))
  if (scale && Number(scale) !== s.scale) s.setScale(Number(scale))
  if (!tour) useApp.getState().setTourOpen(true)
}

