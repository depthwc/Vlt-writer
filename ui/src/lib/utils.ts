import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function debounce<A extends unknown[], R>(fn: (...a: A) => R, ms: number) {
  let t: ReturnType<typeof setTimeout> | undefined
  const d = (...a: A) => {
    if (t) clearTimeout(t)
    t = setTimeout(() => fn(...a), ms)
  }
  /** run it now (what it returns, a save's promise for one) */
  d.flush = (...a: A): R => {
    if (t) clearTimeout(t)
    return fn(...a)
  }
  return d
}

export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))
