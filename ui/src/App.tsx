import * as React from 'react'
import { Toaster } from 'sonner'

import { CommandPalette, Rail, Tour, usePaletteActions } from '@/components/Shell'
import { TitleBar } from '@/components/TitleBar'
import { TooltipProvider } from '@/components/ui/overlay'
import { api, inTauri } from '@/lib/api'
import { useApp } from '@/state/app'
import { fire } from '@/state/bus'
import HandsWorkspace from '@/workspaces/hands/HandsWorkspace'
import PapersWorkspace from '@/workspaces/Papers'
import SettingsWorkspace from '@/workspaces/Settings'
import WriteWorkspace from '@/workspaces/write/WriteWorkspace'

export default function App() {
  const ws = useApp((s) => s.ws)
  const theme = useApp((s) => s.theme)
  // opened on a copy of a library for testing: never mistaken for the real one
  const [testCopy, setTestCopy] = React.useState(false)
  React.useEffect(() => {
    if (inTauri()) void api.testCopy().then(setTestCopy).catch(() => {})
  }, [])
  const actions = usePaletteActions({
    newDoc: () => fire('newDoc'),
    importDoc: () => fire('importDoc'),
    exportDoc: () => {
      useApp.getState().setWs('write')
      useApp.getState().setExportOpen(true)
    },
    printSheet: () => {
      useApp.getState().setWs('hands')
      fire('printSheet')
    },
    addPaper: () => {
      useApp.getState().setWs('papers')
      fire('addPaper')
    },
  })

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return
      const s = useApp.getState()
      const k = e.key.toLowerCase()
      const map: Record<string, () => void> = {
        k: () => s.setPaletteOpen(!s.paletteOpen),
        '1': () => s.setWs('write'),
        '2': () => s.setWs('hands'),
        '3': () => s.setWs('papers'),
        ',': () => s.setWs('settings'),
        n: () => { s.setWs('write'); fire('newDoc') },
        o: () => { s.setWs('write'); fire('importDoc') },
        e: () => { s.setWs('write'); s.setExportOpen(true) },
      }
      if (map[k]) {
        e.preventDefault()
        map[k]()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <TooltipProvider>
      <div className="flex h-screen flex-col">
        <TitleBar testCopy={testCopy} />
        <div className="flex min-h-0 flex-1">
          <Rail />
          <main className="min-w-0 flex-1">
            {ws === 'write' && <WriteWorkspace />}
            {ws === 'hands' && <HandsWorkspace />}
            {ws === 'papers' && <PapersWorkspace />}
            {ws === 'settings' && <SettingsWorkspace />}
          </main>
        </div>
      </div>
      <CommandPalette actions={actions} />
      <Tour />
      <Toaster position="bottom-right" theme={theme} toastOptions={{ style: { fontFamily: 'var(--font-sans)', borderRadius: 10 } }} />
    </TooltipProvider>
  )
}
