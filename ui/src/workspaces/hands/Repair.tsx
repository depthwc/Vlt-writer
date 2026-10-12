import { Eraser, Move3D, Paintbrush, RotateCcw, Scissors, Trash2, Undo2 } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { ZoomPan, useZoom } from '@/components/ZoomPan'
import { Button } from '@/components/ui/button'
import { Input, Row, Segmented, Slider, Switch } from '@/components/ui/controls'
import { api, hw, type Edit, type GlyphView, type P2 } from '@/lib/api'

type Tool = 'baseline' | 'erase' | 'restore' | 'split'

function Overlay({ g, bl, setBl, onBlDone, tool, radius, onStroke, cut, setCut }: {
  g: GlyphView
  bl: number
  setBl: (v: number) => void
  onBlDone: () => void
  tool: Tool
  radius: number
  onStroke: (pts: P2[]) => void
  cut: number | null
  setCut: (x: number) => void
}) {
  const { t } = useTranslation()
  const { scale, toContent } = useZoom()
  const [cursor, setCursor] = React.useState<P2 | null>(null)
  const [pts, setPts] = React.useState<P2[] | null>(null)
  const dragBl = React.useRef(false)
  const brushing = tool === 'erase' || tool === 'restore'
  if (tool === 'split') {
    return (
      <svg className="absolute inset-0 cursor-col-resize overflow-visible" width={g.w} height={g.h} viewBox={`0 0 ${g.w} ${g.h}`} data-handle=""
        role="slider" aria-label={t('hands.repair.split')} aria-valuenow={cut ?? undefined} tabIndex={0}
        onPointerDown={(e) => setCut(Math.round(toContent(e.clientX, e.clientY)[0]))}
        onKeyDown={(e) => {
          if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
            e.preventDefault()
            setCut(Math.max(1, Math.min(g.w - 1, (cut ?? g.w / 2) + (e.key === 'ArrowLeft' ? -1 : 1))))
          }
        }}>
        <rect x={0} y={0} width={g.w} height={g.h} fill="transparent" stroke="var(--line)" strokeWidth={1 / scale} />
        {cut !== null && <line x1={cut} x2={cut} y1={-g.h * 0.1} y2={g.h * 1.1} stroke="var(--redpen)" strokeWidth={1.5 / scale} strokeDasharray={`${5 / scale} ${4 / scale}`} />}
      </svg>
    )
  }
  return (
    <svg className="absolute inset-0 overflow-visible" width={g.w} height={g.h} viewBox={`0 0 ${g.w} ${g.h}`}
      style={{ cursor: brushing ? 'none' : 'default' }}
      data-handle={brushing ? '' : undefined}
      onPointerMove={(e) => {
        const p = toContent(e.clientX, e.clientY)
        setCursor(p)
        if (dragBl.current) setBl(Math.max(0, Math.min(g.h, p[1])))
        else if (pts) setPts([...pts, p])
      }}
      onPointerLeave={() => setCursor(null)}
      onPointerDown={(e) => {
        if (!brushing) return
        e.stopPropagation()
        ;(e.currentTarget as Element).setPointerCapture(e.pointerId)
        setPts([toContent(e.clientX, e.clientY)])
      }}
      onPointerUp={() => {
        if (dragBl.current) {
          dragBl.current = false
          onBlDone()
        }
        if (pts && pts.length) onStroke(pts)
        setPts(null)
      }}
    >
      <rect x={0} y={0} width={g.w} height={g.h} fill="none" stroke="var(--line)" strokeWidth={1 / scale} />
      {/* baseline: the red pen line the letter sits on */}
      <g data-handle role="slider" aria-label={t('hands.repair.baseline')} aria-valuenow={Math.round(bl)} tabIndex={0}
        className="cursor-ns-resize outline-none"
        onPointerDown={(e) => {
          if (brushing) return
          e.stopPropagation()
          dragBl.current = true
          ;(e.currentTarget.ownerSVGElement as Element).setPointerCapture(e.pointerId)
        }}
        onKeyDown={(e) => {
          if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            e.preventDefault()
            setBl(bl + (e.key === 'ArrowUp' ? -1 : 1) * (e.shiftKey ? 5 : 1))
          }
        }}
        onKeyUp={(e) => (e.key === 'ArrowUp' || e.key === 'ArrowDown') && onBlDone()}
      >
        <line x1={-g.w * 0.3} x2={g.w * 1.3} y1={bl} y2={bl} stroke="transparent" strokeWidth={16 / scale} />
        <line x1={-g.w * 0.3} x2={g.w * 1.3} y1={bl} y2={bl} stroke="var(--redpen)" strokeWidth={1.5 / scale} strokeDasharray={`${6 / scale} ${4 / scale}`} />
      </g>
      {pts && pts.length > 0 && (
        <polyline points={pts.map((p) => p.join(',')).join(' ')} fill="none" strokeLinecap="round" strokeLinejoin="round"
          stroke={tool === 'erase' ? 'var(--paper-2)' : 'var(--ink)'} strokeOpacity={tool === 'erase' ? 0.85 : 0.35} strokeWidth={radius * 2} />
      )}
      {brushing && cursor && <circle cx={cursor[0]} cy={cursor[1]} r={radius} fill="none" stroke="var(--ink)" strokeWidth={1.25 / scale} />}
    </svg>
  )
}

export function RepairPanel({ hand, g, onChanged, onDeleted }: { hand: string; g: GlyphView; onChanged: () => void; onDeleted: () => void }) {
  const { t } = useTranslation()
  const [edits, setEdits] = React.useState<Edit[]>([])
  const [tool, setTool] = React.useState<Tool>('baseline')
  const [radius, setRadius] = React.useState(6)
  const [bl, setBl] = React.useState(g.bl)
  const [label, setLabel] = React.useState(g.text)
  const [v, setV] = React.useState(0)
  const [cut, setCut] = React.useState<number | null>(null)
  const chars = [...g.text]
  const [leftText, setLeftText] = React.useState(chars[0] ?? '')
  const [rightText, setRightText] = React.useState(chars.slice(1).join(''))

  React.useEffect(() => {
    setBl(g.bl)
    setLabel(g.text)
    void api.glyphEdits(hand, g.id).then(setEdits)
  }, [hand, g.id, g.bl, g.text])

  const save = async (patch: Parameters<typeof api.updateGlyph>[2]) => {
    try {
      await api.updateGlyph(hand, g.id, patch)
      setV((x) => x + 1)
      onChanged()
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    }
  }

  // transforms first, brush strokes after: strokes are in the coordinates the user sees
  const tf = (op: 'rotate' | 'slant' | 'scale') => edits.find((e) => e.op === op)
  const brushes = edits.filter((e) => e.op === 'erase' || e.op === 'restore')
  const compose = (rot: number, sl: number, sc: number, br: Edit[]): Edit[] => [
    ...(rot ? [{ op: 'rotate', deg: rot } as Edit] : []),
    ...(sl ? [{ op: 'slant', k: sl } as Edit] : []),
    ...(sc !== 1 ? [{ op: 'scale', f: sc } as Edit] : []),
    ...br,
  ]
  const rot = (tf('rotate') as { deg: number } | undefined)?.deg ?? 0
  const sl = (tf('slant') as { k: number } | undefined)?.k ?? 0
  const sc = (tf('scale') as { f: number } | undefined)?.f ?? 1
  const setEditsSave = (e: Edit[]) => {
    setEdits(e)
    void save({ edits: e })
  }
  const timer = React.useRef<ReturnType<typeof setTimeout>>(undefined)
  const setTransform = (r: number, s: number, c: number) => {
    const e = compose(r, s, c, brushes)
    setEdits(e)
    clearTimeout(timer.current)
    timer.current = setTimeout(() => void save({ edits: e }), 250)
  }

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest('input, textarea, [contenteditable]')) return
      if (e.key === 'e') setTool('erase')
      else if (e.key === 'r') setTool('restore')
      else if (e.key === 'b') setTool('baseline')
      else if (e.key === '[') setRadius((r) => Math.max(1, r - 1))
      else if (e.key === ']') setRadius((r) => Math.min(40, r + 1))
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <div className="flex h-full flex-col">
      <div className="h-[46%] min-h-56 border-b border-line">
        <ZoomPan width={g.w} height={g.h} fitKey={g.id} label={`${t('hands.repair.title')} ${g.text}`} padding={40}>
          <img src={hw(`glyph/${hand}/${g.id}.png`, { v: `${v}-${edits.length}` })} width={g.w} height={g.h} alt={g.text} draggable={false} className="glyph-img block select-none bg-surface" />
          <Overlay g={g} bl={bl} setBl={setBl} onBlDone={() => save({ bl })} tool={tool} radius={radius} cut={cut} setCut={setCut}
            onStroke={(pts) => setEditsSave([...edits, { op: tool === 'erase' ? 'erase' : 'restore', pts: pts.map(([x, y]) => [Math.round(x * 10) / 10, Math.round(y * 10) / 10] as P2), r: radius }])} />
        </ZoomPan>
      </div>
      <div className="flex-1 space-y-1 overflow-auto px-4 py-3">
        <Segmented<Tool> label={t('hands.repair.brush')} value={tool} onChange={setTool} className="mb-2 w-full" options={[
          { value: 'baseline', label: <><Move3D />{t('hands.repair.baseline')}</>, title: 'B' },
          { value: 'erase', label: <><Eraser />{t('hands.repair.erase')}</>, title: 'E' },
          { value: 'restore', label: <><Paintbrush />{t('hands.repair.restore')}</>, title: 'R' },
          { value: 'split', label: <><Scissors /></>, title: t('hands.repair.split') },
        ]} />
        {tool === 'baseline' && <p className="pb-1 text-[0.8rem] text-muted">{t('hands.repair.baselineHint')}</p>}
        {(tool === 'erase' || tool === 'restore') && (
          <Row label={t('hands.repair.brush')}><Slider label={t('hands.repair.brush')} value={radius} min={1} max={40} step={1} onChange={setRadius} format={(x) => `${x}px`} /></Row>
        )}
        {tool === 'split' && (
          <div className="space-y-2 pb-2">
            <p className="text-[0.8rem] text-muted">{t('hands.repair.splitHint')}</p>
            <div className="flex items-center gap-2">
              <Input value={leftText} onChange={(e) => setLeftText(e.target.value)} maxLength={3} className="w-16 text-[1rem]" aria-label={t('hands.repair.left')} />
              <Input value={rightText} onChange={(e) => setRightText(e.target.value)} maxLength={3} className="w-16 text-[1rem]" aria-label={t('hands.repair.right')} />
              <Button size="sm" variant="primary" disabled={cut === null || (!leftText.trim() && !rightText.trim())}
                onClick={async () => { await api.splitGlyph(hand, g.id, cut!, leftText, rightText); onDeleted() }}>
                <Scissors />{t('hands.repair.split')}
              </Button>
            </div>
          </div>
        )}
        <Row label={t('hands.repair.rotate')}><Slider label={t('hands.repair.rotate')} value={rot} min={-15} max={15} step={0.5} onChange={(x) => setTransform(x, sl, sc)} format={(x) => `${x}°`} /></Row>
        <Row label={t('hands.repair.slant')}><Slider label={t('hands.repair.slant')} value={sl} min={-0.4} max={0.4} step={0.02} onChange={(x) => setTransform(rot, x, sc)} format={(x) => x.toFixed(2)} /></Row>
        <Row label={t('hands.repair.scale')}><Slider label={t('hands.repair.scale')} value={sc} min={0.7} max={1.3} step={0.02} onChange={(x) => setTransform(rot, sl, x)} format={(x) => `${Math.round(x * 100)}%`} /></Row>
        <Row label={t('hands.repair.label')} htmlFor="g-label">
          <Input id="g-label" value={label} maxLength={4} className="w-24 text-[1rem]" onChange={(e) => setLabel(e.target.value)} onBlur={() => label.trim() && label !== g.text && save({ text: label })} onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()} />
        </Row>
        <Row label={t('hands.repair.use')}>
          <Switch checked={g.on} onCheckedChange={(on) => save({ on })} label={t('hands.repair.use')} />
        </Row>
        {/\p{L}/u.test(g.text.replace(/[ʻʼ]/g, '')) && (
          <Row label={t('hands.repair.print')}>
            <Switch checked={g.print} onCheckedChange={async (print) => { await api.setGlyphsPrint(hand, [g.id], print); onChanged() }} label={t('hands.repair.print')} />
          </Row>
        )}
        {g.flags.length > 0 && (
          <ul className="space-y-1 py-1 text-[0.8rem] text-redpen">
            {g.flags.map((f) => <li key={f}>{t(`hands.flags.${f}`)}</li>)}
          </ul>
        )}
        <p className="text-[0.75rem] text-muted">{t(`hands.repair.from.${g.src}`)}{g.how ? `: ${t(`hands.repair.how.${g.how}`, { from: g.from })}` : ''}, {g.w}×{g.h}px</p>
        {g.src === 'auto' && <p className="text-[0.75rem] text-muted">{t('hands.repair.autoNote')}</p>}
        <div className="flex flex-wrap gap-2 pt-2">
          <Button size="sm" disabled={!edits.length} onClick={() => setEditsSave(brushes.length ? edits.slice(0, -1) : compose(0, 0, 1, []))}><Undo2 />{t('hands.repair.undoEdit')}</Button>
          <Button size="sm" disabled={!edits.length} onClick={() => setEditsSave([])}><RotateCcw />{t('hands.repair.resetEdits')}</Button>
          <Button size="sm" variant="danger" onClick={async () => { await api.deleteGlyphs(hand, [g.id]); onDeleted() }}><Trash2 />{t('hands.repair.remove')}</Button>
        </div>
      </div>
    </div>
  )
}
