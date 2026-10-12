import { getCurrentWindow } from '@tauri-apps/api/window'
import { Copy, Minus, Square, X } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'

import { inTauri } from '@/lib/api'
import { cn } from '@/lib/utils'
import { useApp } from '@/state/app'
import { finishBeforeClose } from '@/state/closing'

/** The window's title bar, part of the app: its colours, the open document's name, and the
 *  window buttons. Drag it to move the window, double-click it to maximize. */
export function TitleBar({ testCopy }: { testCopy: boolean }) {
  const { t } = useTranslation()
  const ws = useApp((s) => s.ws)
  const docName = useApp((s) => s.docName)
  const win = React.useMemo(() => (inTauri() ? getCurrentWindow() : null), [])
  const [max, setMax] = React.useState(false)
  const title = ws === 'write' && docName?.trim() ? `${docName.trim()} · Vlt-writer` : 'Vlt-writer'

  // the same name on the taskbar
  React.useEffect(() => {
    document.title = title
    void win?.setTitle(title).catch(() => {})
  }, [title, win])

  React.useEffect(() => {
    if (!win) return
    const offs: (() => void)[] = []
    const check = () => void win.isMaximized().then(setMax).catch(() => {})
    check()
    void win.onResized(check).then((f) => offs.push(f))
    // closing (this button or Alt+F4): the last edits are saved first
    void win.onCloseRequested(async () => {
      await finishBeforeClose()
    }).then((f) => offs.push(f))
    return () => offs.forEach((f) => f())
  }, [win])

  const button = 'flex h-full w-[46px] items-center justify-center text-graphite transition-colors hover:bg-ink-soft'
  return (
    <header data-tauri-drag-region className="flex h-9 shrink-0 select-none items-center border-b border-line bg-surface pl-3 text-[0.8rem]">
      <span data-tauri-drag-region className="min-w-0 truncate text-muted">{title}</span>
      {testCopy && (
        <span data-tauri-drag-region className="ml-3 shrink-0 rounded bg-redpen px-1.5 py-0.5 text-[0.7rem] font-semibold text-white">TEST COPY — not your library</span>
      )}
      <div data-tauri-drag-region className="h-full min-w-8 flex-1" />
      {win && (
        <div className="flex h-full">
          <button className={button} onClick={() => void win.minimize()} aria-label={t('window.minimize')} title={t('window.minimize')}>
            <Minus className="size-4" strokeWidth={1.5} />
          </button>
          <button className={button} onClick={() => void win.toggleMaximize()} aria-label={max ? t('window.restore') : t('window.maximize')} title={max ? t('window.restore') : t('window.maximize')}>
            {max ? <Copy className="size-3.5 -scale-x-100" strokeWidth={1.5} /> : <Square className="size-3.5" strokeWidth={1.5} />}
          </button>
          <button className={cn(button, 'hover:bg-redpen hover:text-white')} onClick={() => void win.close()} aria-label={t('window.close')} title={t('window.close')}>
            <X className="size-4" strokeWidth={1.5} />
          </button>
        </div>
      )}
    </header>
  )
}
