import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { StrictMode } from 'react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import '@/i18n'
import type { GlyphView, HandView } from '@/lib/api'

import { LetterTree, letterGroups } from './LetterTree'

const glyph = (id: string, text: string): GlyphView => ({ id, text, src: 'text', w: 10, h: 10, bl: 8, on: true, flags: [], edits: 0, how: null, from: '', print: false })
const view = {
  meta: { id: 'h' },
  glyphs: [glyph('1', 'a'), glyph('2', 'a'), glyph('3', 'б'), glyph('4', 'б'), glyph('5', 'б')],
  coverage: [
    { group: 'latin', chars: [{ c: 'a', n: 2 }, { c: 'b', n: 0 }] },
    { group: 'cyrillic', chars: [{ c: 'б', n: 3 }] },
  ],
  refill: 1,
} as unknown as HandView

function Tree({ picking = false, picked = new Set<string>() }: { picking?: boolean; picked?: Set<string> }) {
  return <LetterTree hand="h" groups={letterGroups(view, 'all', '')} v={0} sel={null} onSel={() => {}} picking={picking} picked={picked} onPick={() => {}} openAll={false} onFill={() => {}} />
}

const header = (name: RegExp) => screen.getAllByRole('button').find((b) => b.closest('section') && name.test(b.textContent ?? '') && b.parentElement?.tagName === 'SECTION')!
const stored = (k: string) => JSON.parse(localStorage.getItem(`hw.${k}`) ?? 'null')

describe('the characters tree keeps what was left open or shut', () => {
  beforeEach(() => localStorage.clear())
  afterEach(cleanup)

  for (const strict of [false, true]) {
    it(`a shut section stays shut${strict ? ' (strict mode)' : ''}`, () => {
      const wrap = (el: React.ReactElement) => (strict ? <StrictMode>{el}</StrictMode> : el)
      const r = render(wrap(<Tree />))
      fireEvent.click(header(/Latin letters/))
      expect(stored('tree.shut')).toEqual(['latin'])
      // a row opened, versions picked, the view refreshed: the section stays shut
      fireEvent.click(screen.getByRole('button', { name: /^б/ }))
      expect(stored('tree.open')).toEqual(['cyrillic:б'])
      r.rerender(wrap(<Tree picking picked={new Set(['3'])} />))
      r.rerender(wrap(<Tree />))
      expect(stored('tree.shut')).toEqual(['latin'])
      // and after a restart
      r.unmount()
      render(wrap(<Tree />))
      expect(header(/Latin letters/).getAttribute('aria-expanded')).toBe('false')
      expect(screen.getAllByRole('button', { name: /^б/ }).find((x) => x.hasAttribute('aria-expanded'))?.getAttribute('aria-expanded')).toBe('true')
    })
  }
})
