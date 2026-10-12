import * as React from 'react'
import { create } from 'zustand'

/** One-shot requests from the command palette / shortcuts / other workspaces to a workspace. */
type Req = 'newDoc' | 'importDoc' | 'printSheet' | 'printRefill' | 'addPaper'

export const useBus = create<Record<Req, number>>(() => ({ newDoc: 0, importDoc: 0, printSheet: 0, printRefill: 0, addPaper: 0 }))

export const fire = (r: Req) => useBus.setState((s) => ({ [r]: s[r] + 1 }))

const handled: Partial<Record<Req, number>> = {}

/** Run `fn` once per fired request, also when the workspace mounts after the request. */
export function useRequest(r: Req, fn: () => void) {
  const n = useBus((s) => s[r])
  const f = React.useRef(fn)
  f.current = fn
  React.useEffect(() => {
    if (n > (handled[r] ?? 0)) {
      handled[r] = n
      f.current()
    }
  }, [n, r])
}
