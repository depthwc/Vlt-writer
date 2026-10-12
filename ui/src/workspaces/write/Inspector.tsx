import { ChevronRight, Dices } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input, Row, Segmented, Select, Slider, Switch } from '@/components/ui/controls'
import { Tip } from '@/components/ui/overlay'
import { NOTEBOOK, RANDOM, type LayoutInfo, type Letters, type PaperMeta, type Params, type PenKind, type Project, type Summary } from '@/lib/api'
import { cn } from '@/lib/utils'
import { usePref } from '@/state/prefs'

function Group({ id: key, title, children, open: initial = true }: { id: string; title: string; children: React.ReactNode; open?: boolean }) {
  const [open, setOpen] = usePref(`inspector.${key}`, initial)
  const id = React.useId()
  return (
    <section className="border-b border-line">
      <h2>
        <button aria-expanded={open} aria-controls={id} onClick={() => setOpen(!open)} className="flex h-10 w-full items-center gap-1.5 px-4 text-left text-[0.85rem] font-semibold hover:bg-paper-2">
          <ChevronRight className={cn('size-4 text-muted transition-transform', open && 'rotate-90')} />
          {title}
        </button>
      </h2>
      {open && <div id={id} className="px-4 pb-3">{children}</div>}
    </section>
  )
}

// (a fountain pen came out as a ballpoint: not offered)
const PENS: PenKind[] = ['mine', 'ballpoint', 'gel', 'pencil', 'felt']

function Swatch({ color, label, active, onClick }: { color: string; label: string; active: boolean; onClick: () => void }) {
  return (
    <Tip label={label}>
      <button onClick={onClick} aria-label={label} aria-pressed={active}
        className={cn('size-6 rounded-full border border-line', active && 'ring-2 ring-ink ring-offset-2 ring-offset-surface')}
        style={{ background: color }} />
    </Tip>
  )
}

const rgb = (c: [number, number, number]) => `rgb(${c.map((v) => Math.round(v * 255)).join(' ')})`
const hex = (c: [number, number, number]) => `#${c.map((v) => Math.round(v * 255).toString(16).padStart(2, '0')).join('')}`
const fromHex = (h: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255) as [number, number, number]

export function Inspector({ project, update, hands, papers, info, mode, preview, setPreview, presets, handInk }: {
  project: Project
  update: (p: Partial<Project>) => void
  hands: Summary[]
  papers: PaperMeta[]
  info: LayoutInfo | null
  mode: 'photo' | 'svg'
  preview: { random: boolean; seed: number }
  setPreview: (p: { random: boolean; seed: number }) => void
  presets: [string, [number, number, number]][]
  handInk: [number, number, number] | null
}) {
  const { t } = useTranslation()
  const p = project.params
  const set = <K extends keyof Params>(k: K, v: Params[K]) => update({ params: { ...p, [k]: v } })
  // one paper: which of its photos
  const notebook = project.paper === NOTEBOOK
  const random = project.paper === RANDOM
  const pid = notebook || random ? null : project.paper
  const pl = (pid && project.place[pid]) || {}
  const setPlace = (patch: Record<string, unknown>) => pid && update({ place: { ...project.place, [pid]: { ...pl, ...patch } } })
  const hasNotebook = papers.some((x) => x.side === 'left' || x.side === 'right')
  const pen = project.pen
  const st = project.shot
  return (
    <div className="text-[0.85rem]">
      <Group id="hand" title={t('inspector.hand')}>
        <Select label={t('inspector.hand')} value={project.hand ?? ''} onValueChange={(hand) => update({ hand })} options={hands.map((h) => ({ value: h.id, label: h.name }))} />
      </Group>
      <Group id="paper" title={t('inspector.paper')}>
        <Select label={t('inspector.paper')} value={project.paper ?? ''} onValueChange={(paper) => update({ paper })}
          options={[
            ...(hasNotebook || notebook ? [{ value: NOTEBOOK, label: t('inspector.notebook') }] : []),
            ...(papers.length > 1 || random ? [{ value: RANDOM, label: t('inspector.random') }] : []),
            ...papers.map((x) => ({ value: x.id, label: x.side === 'any' ? x.name : `${x.name} · ${t(`papers.sides.${x.side}`)}` })),
          ]} />
        {(notebook || random) && <p className="mt-1.5 text-[0.8rem] text-muted">{t(notebook ? 'inspector.notebookHint' : 'inspector.randomHint')}</p>}
        {pid && info && info.shots > 1 && (
          <Row label={t('inspector.shot')}>
            <Select label={t('inspector.shot')} value={String(pl.shot ?? 0)} onValueChange={(v) => setPlace({ shot: Number(v), mesh: null })} options={Array.from({ length: info.shots }, (_, i) => ({ value: String(i), label: `${i + 1}` }))} />
          </Row>
        )}
      </Group>
      <Group id="style" title={t('inspector.style')}>
        <Row label={t('inspector.letters')} hint={t('inspector.lettersHint')} stack>
          <Segmented<Letters> label={t('inspector.letters')} value={p.letters ?? 'auto'} onChange={(v) => set('letters', v)} options={[
            { value: 'auto', label: t('inspector.lettersAuto') },
            { value: 'joined', label: t('inspector.lettersJoined') },
            { value: 'print', label: t('inspector.lettersPrint') },
          ]} />
        </Row>
        <Row label={t('inspector.size')}><Slider label={t('inspector.size')} value={p.size} min={0.6} max={1.6} step={0.02} onChange={(v) => set('size', v)} format={(v) => `${Math.round(v * 100)}%`} /></Row>
        <Row label={t('inspector.mess')}><Slider label={t('inspector.mess')} value={p.mess} min={0} max={2} step={0.05} onChange={(v) => set('mess', v)} format={(v) => v.toFixed(2)} /></Row>
        <Row label={t('inspector.joins')} hint={t('inspector.joinsHint')}><Slider label={t('inspector.joins')} value={p.joins} min={0} max={2.5} step={0.05} onChange={(v) => set('joins', v)} format={(v) => `${Math.round(v * 100)}%`} /></Row>
        <Row label={t('inspector.slant')}><Slider label={t('inspector.slant')} value={p.slant} min={-15} max={15} step={0.5} onChange={(v) => set('slant', v)} format={(v) => `${v}°`} /></Row>
        <Row label={t('inspector.wordSpacing')}><Slider label={t('inspector.wordSpacing')} value={p.word_spacing} min={0.6} max={2} step={0.02} onChange={(v) => set('word_spacing', v)} format={(v) => `${Math.round(v * 100)}%`} /></Row>
        <Row label={t('inspector.drift')}><Slider label={t('inspector.drift')} value={p.line_drift} min={0} max={2.5} step={0.05} onChange={(v) => set('line_drift', v)} format={(v) => v.toFixed(2)} /></Row>
        <Row label={t('inspector.indent')}><Slider label={t('inspector.indent')} value={p.indent_mm} min={0} max={30} step={0.5} onChange={(v) => set('indent_mm', v)} format={(v) => `${v} mm`} /></Row>
        <Row label={t('inspector.seed')}>
          <div className="flex gap-1.5">
            <Input type="number" value={p.seed} onChange={(e) => set('seed', Math.max(0, Number(e.target.value)))} aria-label={t('inspector.seed')} />
            <Tip label={t('inspector.newSeed')}>
              <Button size="icon" onClick={() => update({ params: { ...p, seed: Math.floor(Math.random() * 1e6), overrides: {} } })} aria-label={t('inspector.newSeed')}><Dices /></Button>
            </Tip>
          </div>
        </Row>
      </Group>
      <Group id="pen" title={t('inspector.pen')}>
        <Row label={t('inspector.penKind')}>
          <Select label={t('inspector.penKind')} value={pen.kind} onValueChange={(kind) => update({ pen: { ...pen, kind: kind as PenKind } })} options={PENS.map((k) => ({ value: k, label: t(`inspector.pens.${k}`) }))} />
        </Row>
        <Row label={t('inspector.inkColor')}>
          <div className="flex flex-wrap items-center gap-2 py-1">
            <Swatch color={handInk ? rgb(handInk) : 'var(--ink)'} label={t('inspector.colors.mine')} active={pen.color === null} onClick={() => update({ pen: { ...pen, color: null } })} />
            {presets.map(([name, c]) => (
              <Swatch key={name} color={rgb(c)} label={t(`inspector.colors.${name}`)} active={pen.color?.join() === c.join()} onClick={() => update({ pen: { ...pen, color: c } })} />
            ))}
            <label className="relative size-6 cursor-pointer overflow-hidden rounded-full border border-dashed border-line" title={t('inspector.colors.custom')}>
              <span className="sr-only">{t('inspector.colors.custom')}</span>
              <input type="color" className="absolute -inset-2 size-10 cursor-pointer opacity-0" value={hex(pen.color ?? handInk ?? [0.15, 0.2, 0.5])} onChange={(e) => update({ pen: { ...pen, color: fromHex(e.target.value) } })} />
            </label>
          </div>
        </Row>
        <Row label={t('inspector.density')}><Slider label={t('inspector.density')} value={pen.density} min={0.5} max={1.5} step={0.02} onChange={(v) => update({ pen: { ...pen, density: v } })} format={(v) => `${Math.round(v * 100)}%`} /></Row>
        <Row label={t('inspector.inkLow')}><Slider label={t('inspector.inkLow')} value={pen.ink_low} min={0} max={1} step={0.05} onChange={(v) => update({ pen: { ...pen, ink_low: v } })} format={(v) => `${Math.round(v * 100)}%`} /></Row>
      </Group>
      <Group id="realism" title={t('inspector.realism')}>
        <Row label={t('inspector.mistakes')} hint={t('inspector.mistakesHint')}><Slider label={t('inspector.mistakes')} value={p.mistakes} min={0} max={5} step={0.25} onChange={(v) => set('mistakes', v)} format={(v) => v.toFixed(2)} /></Row>
        <Row label={t('exportDlg.strength')} hint={t('inspector.strengthHint')}><Slider label={t('exportDlg.strength')} value={st.strength} min={0} max={1.5} step={0.05} onChange={(v) => update({ shot: { ...st, strength: v } })} format={(v) => `${Math.round(v * 100)}%`} /></Row>
        <Row label={t('inspector.tilt')} hint={t('inspector.tiltHint')}><Slider label={t('inspector.tilt')} value={st.angle_max ?? 4} min={0} max={10} step={0.5} onChange={(v) => update({ shot: { ...st, angle_max: v } })} format={(v) => (v ? `${v}°` : t('inspector.off'))} /></Row>
        {mode === 'photo' && (
          <div className="flex items-center justify-between pt-2">
            <label className="flex items-center gap-2"><Switch checked={preview.random} onCheckedChange={(random) => setPreview({ ...preview, random })} label={t('inspector.previewRandom')} /><span className="text-[0.8rem]">{t('inspector.previewRandom')}</span></label>
            {preview.random && <Button size="icon-sm" variant="ghost" onClick={() => setPreview({ ...preview, seed: preview.seed + 1 })} aria-label={t('inspector.newSeed')}><Dices /></Button>}
          </div>
        )}
      </Group>
    </div>
  )
}


