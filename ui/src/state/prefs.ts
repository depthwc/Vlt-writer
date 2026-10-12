import * as React from 'react'

import { local, persist } from './app'

function read<T>(key: string, initial: T): T {
  const s = local(key)
  if (s === null) return initial
  try {
    return JSON.parse(s) as T
  } catch {
    return initial
  }
}

/** A choice made in the interface (a section left open or shut, a view), kept between runs. */
export function usePref<T>(key: string, initial: T): [T, (v: T | ((prev: T) => T)) => void] {
  const [value, setValue] = React.useState<T>(() => read(key, initial))
  // the value now, for a change made from the one before: worked out and kept here once, never
  // inside a state update (React may run those twice)
  const now = React.useRef(value)
  const set = React.useCallback((v: T | ((prev: T) => T)) => {
    const next = typeof v === 'function' ? (v as (p: T) => T)(now.current) : v
    now.current = next
    persist(key, JSON.stringify(next))
    setValue(next)
  }, [key])
  return [value, set]
}
