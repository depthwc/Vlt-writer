import { open, save } from '@tauri-apps/plugin-dialog'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import * as React from 'react'

import { inTauri } from './api'

export const IMAGE_EXT = ['jpg', 'jpeg', 'png', 'webp', 'heic', 'bmp', 'tif', 'tiff']
export const DOC_EXT = ['pdf', 'docx', 'odt', 'rtf', 'html', 'htm', 'md', 'markdown', 'txt']

export async function pickImages(multiple: boolean): Promise<string[]> {
  const r = await open({ multiple, filters: [{ name: 'Images', extensions: IMAGE_EXT }] })
  return r === null ? [] : Array.isArray(r) ? r : [r]
}

export async function pickFile(name: string, extensions: string[]): Promise<string | null> {
  const r = await open({ multiple: false, filters: [{ name, extensions }] })
  return typeof r === 'string' ? r : null
}

export async function pickFolder(): Promise<string | null> {
  const r = await open({ directory: true, multiple: false })
  return typeof r === 'string' ? r : null
}

export async function saveAs(defaultPath: string, name: string, extensions: string[]): Promise<string | null> {
  return save({ defaultPath, filters: [{ name, extensions }] })
}

/** Files dropped onto the window while `active`; paths filtered by extension. */
export function useFileDrop(active: boolean, exts: string[], onDrop: (paths: string[]) => void) {
  const [over, setOver] = React.useState(false)
  const cb = React.useRef(onDrop)
  cb.current = onDrop
  React.useEffect(() => {
    if (!active || !inTauri()) return
    let un: (() => void) | undefined
    void getCurrentWebview()
      .onDragDropEvent((e) => {
        const p = e.payload
        if (p.type === 'enter' || p.type === 'over') setOver(true)
        else if (p.type === 'leave') setOver(false)
        else if (p.type === 'drop') {
          setOver(false)
          const ok = p.paths.filter((f) => exts.includes(f.split('.').pop()?.toLowerCase() ?? ''))
          if (ok.length) cb.current(ok)
        }
      })
      .then((u) => (un = u))
    return () => un?.()
  }, [active, exts])
  return over
}

export const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p
