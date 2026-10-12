import { Check, ChevronRight, NotebookPen } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/controls'
import { hw, type GlyphView, type HandView } from '@/lib/api'
import { cn } from '@/lib/utils'
import { usePref } from '@/state/prefs'

export type Filter = 'all' | 'flagged' | 'off'

export interface Letter { text: string; glyphs: GlyphView[] }
/** A section: its written characters, and the ones not written yet (places to fill). */
export interface Group { key: string; letters: Letter[]; empty: string[]; have: number; total: number }

/** The written versions in the coverage's sections (Latin, Latin print, Cyrillic, Cyrillic
 *  print, digits and marks, maths, then the rest), each character in alphabet order; a
 *  character of several (oʻ) follows its first letter. Every character of a section shows,
 *  the ones not written yet as empty places. Filtered by the view and the search. */
export function letterGroups(view: HandView, filter: Filter, find: string): Group[] {
  const groupOf = new Map<string, string>()
  const order = new Map<string, number>()
  view.coverage.forEach((g) => g.chars.forEach(({ c }) => {
    if (!groupOf.has(c)) groupOf.set(c, g.group)
    if (!order.has(c)) order.set(c, order.size)
  }))
  const keys = new Set(view.coverage.map((g) => g.group))
  // apostrophes typed any way find oʻ, gʻ and the tutuq belgisi ʼ
  const plain = (s: string) => s.toLowerCase().replace(/[ʻʼ’‘`´]/g, "'")
  const q = plain(find.trim())
  const match = (s: string) => !q || plain(s).includes(q)
  const by = new Map<string, Map<string, GlyphView[]>>()
  for (const g of view.glyphs) {
    if (filter === 'flagged' && !g.flags.length) continue
    if (filter === 'off' && g.on) continue
    if (!match(g.text)) continue
    let key = groupOf.get([...g.text][0]) ?? 'other'
    // print letters have sections of their own
    if (g.print && keys.has(`${key}_print`)) key = `${key}_print`
    const m = by.get(key) ?? new Map<string, GlyphView[]>()
    m.set(g.text, [...(m.get(g.text) ?? []), g])
    by.set(key, m)
  }
  const rank = (t: string) => (order.get([...t][0]) ?? 9e8) + ([...t].length > 1 ? 0.5 : 0)
  const out: Group[] = []
  for (const cg of [...view.coverage, { group: 'other', chars: [] }]) {
    const m = by.get(cg.group) ?? new Map<string, GlyphView[]>()
    const letters = [...m.entries()].map(([text, glyphs]) => ({ text, glyphs })).sort((a, b) => rank(a.text) - rank(b.text) || a.text.localeCompare(b.text))
    const empty = filter === 'all' ? cg.chars.filter((c) => c.n === 0 && !m.has(c.c) && match(c.c)).map((c) => c.c) : []
    if (!letters.length && !empty.length) continue
    out.push({ key: cg.group, letters, empty, have: cg.chars.filter((c) => c.n > 0).length, total: cg.chars.length })
  }
  return out
}

function Tile({ hand, g, v, current, picking, picked, onClick }: { hand: string; g: GlyphView; v: number; current: boolean; picking: boolean; picked: boolean; onClick: (e: React.MouseEvent) => void }) {
  const { t } = useTranslation()
  const flagged = g.flags.length > 0
  const label = [g.text, t(`hands.repair.from.${g.src}`), ...g.flags.map((f) => t(`hands.flags.${f}`)), g.on ? '' : t('common.off')].filter(Boolean).join(', ')
  return (
    <button onClick={onClick} aria-pressed={picking ? picked : current} aria-label={label} title={label}
      className={cn('relative flex h-14 min-w-9 items-end justify-center rounded-md border border-transparent bg-surface px-1 hover:border-line',
        !picking && current && 'border-ink ring-2 ring-ink/25', picked && 'border-ink bg-ink/10', !g.on && 'opacity-30', flagged && 'redpen-ring')}>
      <img src={hw(`glyph/${hand}/${g.id}.png`, { v: `${v}-${g.edits}` })} alt="" loading="lazy" draggable={false} className="glyph-img max-h-[3.1rem] max-w-20 object-contain" />
      {picked && <Check className="absolute right-0.5 top-0.5 size-3.5 rounded-sm bg-ink p-px text-on-ink" strokeWidth={3} aria-hidden />}
    </button>
  )
}

export function LetterTree({ hand, groups, v, sel, onSel, picking, picked, onPick, openAll, onFill }: {
  hand: string
  groups: Group[]
  v: number
  sel: string | null
  onSel: (id: string) => void
  picking: boolean
  picked: Set<string>
  /** set these versions picked or not (ctrl+click picks while not picking too) */
  onPick: (ids: string[], on: boolean) => void
  /** every character open (a search or a short list) */
  openAll: boolean
  /** go and write the characters not written yet */
  onFill: () => void
}) {
  const { t } = useTranslation()
  // sections shut and characters open, as they were left
  const [shutKeys, setShut] = usePref<string[]>('tree.shut', [])
  const [openKeys, setOpen] = usePref<string[]>('tree.open', [])
  const shut = new Set(shutKeys)
  const open = new Set(openKeys)
  const last = React.useRef<{ text: string; i: number } | null>(null)
  const flip = (s: string[], k: string) => (s.includes(k) ? s.filter((x) => x !== k) : [...s, k])

  const click = (l: Letter, i: number, e: React.MouseEvent) => {
    const g = l.glyphs[i]
    if (picking || e.ctrlKey || e.metaKey) {
      // shift: everything from the last one clicked in this character
      if (e.shiftKey && last.current?.text === l.text) {
        const [a, b] = [last.current.i, i].sort((x, y) => x - y)
        onPick(l.glyphs.slice(a, b + 1).map((x) => x.id), true)
      } else {
        onPick([g.id], !picked.has(g.id))
      }
      last.current = { text: l.text, i }
    } else {
      onSel(g.id)
    }
  }

  return (
    <div className="space-y-4">
      {groups.map(({ key, letters, empty, have, total }) => {
        const isOpen = !shut.has(key)
        const versions = letters.reduce((n, l) => n + l.glyphs.length, 0)
        const flagged = letters.reduce((n, l) => n + l.glyphs.filter((g) => g.flags.length).length, 0)
        const name = key === 'other' ? t('hands.other') : t(`hands.sheet.${key}`)
        const rowKey = (text: string) => `${key}:${text}`
        return (
          <section key={key} aria-label={name}>
            <button onClick={() => setShut((s) => flip(s, key))} aria-expanded={isOpen}
              className="mb-1.5 flex w-full items-center gap-2 rounded-md px-1 py-1 text-left hover:bg-surface">
              <ChevronRight className={cn('size-4 shrink-0 text-muted transition-transform', isOpen && 'rotate-90')} />
              <h3 className="text-[0.9rem] font-semibold">{name}</h3>
              {total > 0 && <span className="text-[0.8rem] tabular-nums text-muted">{have}/{total}</span>}
              <span className="text-[0.8rem] tabular-nums text-muted">· {t(versions === 1 ? 'hands.nVersion' : 'hands.nVersions', { n: versions })}</span>
              {flagged > 0 && <span className="text-[0.8rem] tabular-nums text-redpen">{t('hands.toLook', { n: flagged })}</span>}
            </button>
            {isOpen && (
              <>
                <ul className="space-y-1">
                  {letters.map((l) => {
                    const lOpen = openAll || open.has(rowKey(l.text))
                    const lFlagged = l.glyphs.filter((g) => g.flags.length).length
                    const nPicked = l.glyphs.filter((g) => picked.has(g.id)).length
                    return (
                      <li key={l.text} className="rounded-lg border border-line bg-surface">
                        <div className="flex h-11 items-center gap-2 px-2">
                          {picking && (
                            <Checkbox checked={nPicked === l.glyphs.length} label={t('hands.selectLetter', { c: l.text })}
                              onCheckedChange={(on) => onPick(l.glyphs.map((g) => g.id), on)} />
                          )}
                          <button onClick={() => setOpen((s) => flip(s, rowKey(l.text)))} aria-expanded={lOpen} disabled={openAll}
                            className="flex h-full min-w-0 flex-1 items-center gap-2 text-left">
                            <ChevronRight className={cn('size-4 shrink-0 text-muted transition-transform', lOpen && 'rotate-90', openAll && 'opacity-0')} />
                            <span className="w-9 shrink-0 text-[1.05rem] font-semibold">{l.text}</span>
                            <span className="w-12 shrink-0 text-[0.8rem] tabular-nums text-muted">{l.glyphs.length}</span>
                            {lFlagged > 0 && <span className="shrink-0 text-[0.8rem] tabular-nums text-redpen">{t('hands.toLook', { n: lFlagged })}</span>}
                            {picking && nPicked > 0 && <span className="shrink-0 text-[0.8rem] tabular-nums">{t('hands.selected', { n: nPicked })}</span>}
                            {!lOpen && (
                              <span className="flex h-8 min-w-0 flex-1 items-end gap-1 overflow-hidden" aria-hidden>
                                {l.glyphs.slice(0, 14).map((g) => (
                                  <img key={g.id} src={hw(`glyph/${hand}/${g.id}.png`, { v: `${v}-${g.edits}` })} alt="" loading="lazy" draggable={false}
                                    className={cn('glyph-img max-h-8 max-w-12 object-contain', !g.on && 'opacity-30')} />
                                ))}
                              </span>
                            )}
                          </button>
                        </div>
                        {lOpen && (
                          <div className="flex flex-wrap items-end gap-0.5 px-2 pb-2" role="group" aria-label={`${l.text}: ${t('hands.variants', { n: l.glyphs.length })}`}>
                            {l.glyphs.map((g, i) => (
                              <Tile key={g.id} hand={hand} g={g} v={v} current={g.id === sel} picking={picking} picked={picked.has(g.id)} onClick={(e) => click(l, i, e)} />
                            ))}
                          </div>
                        )}
                      </li>
                    )
                  })}
                </ul>
                {empty.length > 0 && (
                  <div className={cn('rounded-lg border border-dashed border-ink/30 bg-surface/70 px-3 py-2', letters.length > 0 && 'mt-1')}>
                    <div className="mb-1.5 flex items-center gap-2">
                      <span className="flex-1 text-[0.8rem] text-muted">{t('hands.notWritten', { n: empty.length })}</span>
                      <Button size="sm" variant="quiet" onClick={onFill}><NotebookPen />{t('hands.writeThem')}</Button>
                    </div>
                    <ul className="flex flex-wrap gap-1" aria-label={t('hands.notWritten', { n: empty.length })}>
                      {empty.map((c) => (
                        <li key={c} className="flex size-9 items-center justify-center rounded-md border border-dashed border-line text-[1rem] text-muted">{c}</li>
                      ))}
                    </ul>
                  </div>
                )}
              </>
            )}
          </section>
        )
      })}
    </div>
  )
}
