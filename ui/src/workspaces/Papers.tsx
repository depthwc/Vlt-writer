import { ImagePlus, Plus, ScanLine, Trash2, X } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { EmptyState, NotebookArt } from '@/components/Shell'
import { Handle, ZoomPan, useZoom } from '@/components/ZoomPan'
import { Button } from '@/components/ui/button'
import { Input, NumberField, Row, Segmented, Select } from '@/components/ui/controls'
import { Dialog, Tip } from '@/components/ui/overlay'
import { api, hw, type P2, type PaperKind, type PaperMeta, type PaperSide, type PaperView } from '@/lib/api'
import { IMAGE_EXT, fileName, pickImages, useFileDrop } from '@/lib/files'
import { applyH, invH, sheetH } from '@/lib/homography'
import { cn } from '@/lib/utils'
import { useApp } from '@/state/app'
import { useRequest } from '@/state/bus'

type Mode = 'corners' | 'lines' | 'area'
const SIDES: PaperSide[] = ['right', 'left', 'any']

/** Overlay colours: fixed, so they show on white paper and its printed lines in either theme. */
const C = { sheet: '#0891b2', line: '#7c3aed', first: '#16a34a', area: '#2563eb', sel: '#db2777' }
type Key = keyof typeof C

/** Heights down the sheet where the writing area's edges bend (the engine's BEND_N). */
const BEND_N = 7
type Bend = [number[], number[]]
const flatBend = (): Bend => [Array(BEND_N).fill(0), Array(BEND_N).fill(0)]
const normBend = (b?: Bend | null): Bend => (b && b[0]?.length === BEND_N && b[1]?.length === BEND_N ? [b[0], b[1]] : flatBend())

/** x shift (mm) of a bent edge at sheet height y. */
function bendAt(offs: number[], y: number, h: number) {
  if (offs.length < 2) return 0
  const t = Math.max(0, Math.min(offs.length - 1, (y / h) * (offs.length - 1)))
  const i = Math.min(Math.floor(t), offs.length - 2)
  return offs[i] + (offs[i + 1] - offs[i]) * (t - i)
}

function Legend({ mode }: { mode: Mode }) {
  const { t } = useTranslation()
  const items: Key[] = mode === 'corners' ? ['sheet', 'line', 'first', 'area'] : mode === 'lines' ? ['line', 'sel', 'area'] : ['line', 'first', 'area']
  return (
    <ul className="pointer-events-none absolute bottom-2 left-2 flex flex-wrap gap-x-3 gap-y-1 rounded-md border border-line bg-surface/90 px-2.5 py-1.5 text-[0.75rem]" aria-label={t('papers.legend.title')}>
      {items.map((k) => (
        <li key={k} className="flex items-center gap-1.5">
          <svg width="18" height="6" aria-hidden><line x1="1" y1="3" x2="17" y2="3" stroke={C[k]} strokeWidth={k === 'first' || k === 'sel' ? 3 : 2} strokeDasharray={k === 'sheet' || k === 'area' ? '4 3' : undefined} /></svg>
          {t(`papers.legend.${k === 'line' && mode === 'lines' ? 'printed' : k}`)}
        </li>
      ))}
    </ul>
  )
}

function useSizes() {
  const [sizes, setSizes] = React.useState<Record<string, [number, number]>>({ A4: [210, 297] })
  React.useEffect(() => {
    void api.paperSizes().then((v) => setSizes(Object.fromEntries(v.map(([n, w, h]) => [n, [w, h] as [number, number]]))))
  }, [])
  return sizes
}

/** mm size of a named size, or of a measured one written "WxH". */
function sizeOf(sizes: Record<string, [number, number]>, name: string): [number, number] {
  if (sizes[name]) return sizes[name]
  const m = /^([\d.]+)x([\d.]+)$/.exec(name)
  return m ? [Number(m[1]), Number(m[2])] : [210, 297]
}

/** Usual distance between ruled lines (mm). */
const pitchOf = (rules: P2[][]) => {
  const ys = rules.map((l) => l[Math.floor(l.length / 2)][1]).sort((a, b) => a - b)
  const gaps = ys.slice(1).map((y, i) => y - ys[i]).sort((a, b) => a - b)
  return gaps.length ? gaps[Math.floor(gaps.length / 2)] : 8
}

function PaperOverlay({ view, shot, size, mode, corners, setCorners, onCornersDone, rules, setRules, onRulesDone, sel, setSel, area, setArea, onAreaDone, bend, setBend, onBendDone, onStart }: {
  view: PaperView
  shot: number
  size: [number, number]
  mode: Mode
  corners: P2[]
  setCorners: (c: P2[]) => void
  onCornersDone: () => void
  rules: P2[][]
  setRules: (r: P2[][]) => void
  onRulesDone: (r?: P2[][]) => void
  sel: number | null
  setSel: (i: number | null) => void
  area: [number, number]
  setArea: (a: [number, number]) => void
  onAreaDone: () => void
  bend: Bend
  setBend: (b: Bend) => void
  onBendDone: () => void
  onStart: (line: number) => void
}) {
  const { t } = useTranslation()
  const { scale, toContent } = useZoom()
  const m = view.meta
  const s = m.shots[shot]
  const H = sheetH(corners, size)
  const Hinv = invH(corners, size)
  const toPx = (p: P2) => applyH(H, p)
  const toMm = (x: number, y: number) => applyH(Hinv, [x, y])
  const line = (pts: P2[]) => pts.map((p) => toPx(p).join(',')).join(' ')
  const drag = React.useRef<{ i: number; y0: number; orig: P2[] } | null>(null)
  const edge = React.useRef<0 | 1 | null>(null)
  const writing = view.lines_px[shot] ?? []
  // an edge of the writing area: its position, bent at BEND_N heights down the sheet
  const edgePts = (k: number): P2[] => bend[k].map((o, i) => [area[k] + o, (i * size[1]) / (BEND_N - 1)] as P2)
  const grip = (k: number) => toPx([area[k] + bendAt(bend[k], size[1] / 2, size[1]), size[1] / 2])
  return (
    <svg className="absolute inset-0" width={s.w} height={s.h} viewBox={`0 0 ${s.w} ${s.h}`}
      onPointerDown={(e) => {
        if (mode !== 'lines' || !e.altKey) return
        e.stopPropagation()
        const [, ym] = toMm(...(toContent(e.clientX, e.clientY) as [number, number]))
        const xs = Array.from({ length: 11 }, (_, k) => (k * size[0]) / 10)
        const next = [...rules, xs.map((xx) => [xx, ym] as P2)].sort((a, b) => a[Math.floor(a.length / 2)][1] - b[Math.floor(b.length / 2)][1])
        setRules(next)
        onRulesDone(next)
      }}
    >
      {mode === 'corners' && <polygon points={corners.map((c) => c.join(',')).join(' ')} fill="none" stroke={C.sheet} strokeWidth={2 / scale} strokeDasharray={`${8 / scale} ${5 / scale}`} />}

      {mode === 'corners' &&
        writing.map((l, i) => (
          <polyline key={i} points={l.map((p) => p.join(',')).join(' ')} fill="none" stroke={i === m.start_line ? C.first : C.line} strokeWidth={(i === m.start_line ? 3 : 1.5) / scale} opacity={i < m.start_line ? 0.35 : 0.9} />
        ))}

      {mode === 'area' &&
        writing.map((l, i) => (
          <g key={i} data-handle role="button" tabIndex={0} aria-label={t('papers.startHere', { n: i + 1 })} aria-pressed={i === m.start_line} className="cursor-pointer outline-none"
            onClick={() => onStart(i)} onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && onStart(i)}>
            <polyline points={l.map((p) => p.join(',')).join(' ')} fill="none" stroke="transparent" strokeWidth={12 / scale} />
            <polyline points={l.map((p) => p.join(',')).join(' ')} fill="none" stroke={i === m.start_line ? C.first : C.line}
              strokeWidth={(i === m.start_line ? 3.5 : 1.5) / scale} opacity={i < m.start_line ? 0.35 : 0.9} />
          </g>
        ))}

      {mode === 'lines' &&
        rules.map((l, i) => (
          <g key={i} data-handle role="button" tabIndex={0} aria-label={`${t('papers.editLines')} ${i + 1}`} aria-pressed={sel === i}
            className="cursor-ns-resize outline-none"
            onFocus={() => setSel(i)}
            onKeyDown={(e) => {
              if (e.key === 'Delete' || e.key === 'Backspace') {
                const next = rules.filter((_, k) => k !== i)
                setRules(next)
                setSel(null)
                onRulesDone(next)
              } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
                e.preventDefault()
                const d = (e.key === 'ArrowUp' ? -1 : 1) * (e.shiftKey ? 1 : 0.2)
                setRules(rules.map((r, k) => (k === i ? r.map(([x, y]) => [x, y + d] as P2) : r)))
              }
            }}
            onKeyUp={(e) => (e.key === 'ArrowUp' || e.key === 'ArrowDown') && onRulesDone()}
            onPointerDown={(e) => {
              if (e.altKey) return
              e.stopPropagation()
              setSel(i)
              drag.current = { i, y0: toMm(...(toContent(e.clientX, e.clientY) as [number, number]))[1], orig: l }
              ;(e.currentTarget as Element).setPointerCapture(e.pointerId)
            }}
            onPointerMove={(e) => {
              const d = drag.current
              if (!d) return
              const dy = toMm(...(toContent(e.clientX, e.clientY) as [number, number]))[1] - d.y0
              setRules(rules.map((r, k) => (k === d.i ? d.orig.map(([xx, yy]) => [xx, yy + dy] as P2) : r)))
            }}
            onPointerUp={() => {
              if (drag.current) onRulesDone()
              drag.current = null
            }}
          >
            <polyline points={line(l)} fill="none" stroke="transparent" strokeWidth={14 / scale} />
            <polyline points={line(l)} fill="none" stroke={sel === i ? C.sel : C.line} strokeWidth={(sel === i ? 3 : 1.5) / scale} />
          </g>
        ))}
      {/* the writing area's edges: draggable in every mode */}
      {[0, 1].map((k) => (
        <g key={k} data-handle role="slider" tabIndex={0} aria-label={k === 0 ? t('inspector.left') : t('inspector.right')} aria-valuenow={Math.round(area[k])} className="cursor-ew-resize outline-none"
          onKeyDown={(e) => {
            const d = e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowRight' ? 1 : 0
            if (!d) return
            e.preventDefault()
            const a: [number, number] = [...area]
            a[k] += d * (e.shiftKey ? 5 : 0.5)
            setArea(a)
          }}
          onKeyUp={(e) => (e.key === 'ArrowLeft' || e.key === 'ArrowRight') && onAreaDone()}
          onPointerDown={(e) => {
            e.stopPropagation()
            edge.current = k as 0 | 1
            ;(e.currentTarget as Element).setPointerCapture(e.pointerId)
          }}
          onPointerMove={(e) => {
            if (edge.current !== k) return
            const [xm, ym] = toMm(...(toContent(e.clientX, e.clientY) as [number, number]))
            const a: [number, number] = [...area]
            a[k] = Math.max(0, Math.min(size[0], xm - bendAt(bend[k], ym, size[1])))
            if (a[1] - a[0] >= 20) setArea(a)
          }}
          onPointerUp={() => {
            if (edge.current !== null) onAreaDone()
            edge.current = null
          }}>
          <polyline points={line(edgePts(k))} fill="none" stroke="transparent" strokeWidth={16 / scale} />
          <polyline points={line(edgePts(k))} fill="none" stroke={C.area} strokeWidth={(mode === 'area' ? 2.5 : 2) / scale} strokeDasharray={`${6 / scale} ${4 / scale}`} />
          {/* a grip half way down: the edge can be dragged (in Writing area its points bend it) */}
          {mode !== 'area' && <rect x={grip(k)[0] - 4 / scale} y={grip(k)[1] - 14 / scale} width={8 / scale} height={28 / scale} rx={4 / scale} fill={C.area} stroke="white" strokeWidth={1.5 / scale} />}
        </g>
      ))}
      {/* the edges' points: drag one to bend the edge there */}
      {mode === 'area' &&
        [0, 1].map((k) =>
          edgePts(k).map((p, i) => {
            const [x, y] = toPx(p)
            return (
              <Handle key={`${k}-${i}`} x={x} y={y} color={C.area} label={`${k === 0 ? t('inspector.left') : t('inspector.right')} · ${i + 1}`}
                onMove={(cx, cy) => {
                  const [xm] = toMm(cx, cy)
                  const b: Bend = [[...bend[0]], [...bend[1]]]
                  b[k][i] = xm - area[k]
                  setBend(b)
                }}
                onDone={onBendDone} />
            )
          }),
        )}

      {/* the selected line's points: drag one to bend the line there */}
      {mode === 'lines' && sel !== null && rules[sel] &&
        rules[sel].map((p, k) => {
          const [x, y] = toPx(p)
          return (
            <Handle key={k} x={x} y={y} color={C.sel} label={t('papers.bendPoint', { n: k + 1 })}
              onMove={(cx, cy) => {
                const [, ym] = toMm(cx, cy)
                setRules(rules.map((r, i) => (i === sel ? r.map((q, j) => (j === k ? [q[0], ym] as P2 : q)) : r)))
              }}
              onDone={() => onRulesDone()} />
          )
        })}

      {mode === 'corners' &&
        corners.map((c, i) => (
          <Handle key={i} x={c[0]} y={c[1]} label={`${t('papers.corners')} ${i + 1}`} shape="square" color={C.sheet}
            onMove={(x, y) => setCorners(corners.map((cc, k) => (k === i ? [x, y] : cc)))}
            onDone={onCornersDone}
          />
        ))}
    </svg>
  )
}

function PaperEditor({ view, onChange }: { view: PaperView; onChange: (v: PaperView) => void }) {
  const { t } = useTranslation()
  const sizes = useSizes()
  const m = view.meta
  const [shot, setShot] = React.useState(0)
  const [mode, setMode] = React.useState<Mode>('corners')
  const [busy, setBusy] = React.useState(false)
  const s = m.shots[Math.min(shot, m.shots.length - 1)]
  const [corners, setCorners] = React.useState<P2[]>(s.corners)
  const [rules, setRules] = React.useState<P2[][]>(s.rules)
  const [area, setArea] = React.useState<[number, number]>([m.left_mm, m.right_mm])
  const [bend, setBend] = React.useState<Bend>(normBend(s.area_bend))
  const [sel, setSel] = React.useState<number | null>(null)
  const [confirm, setConfirm] = React.useState(false)
  const [name, setName] = React.useState(m.name)
  const v = `${m.id}-${shot}-${s.file}`
  React.useEffect(() => {
    setCorners(s.corners)
    setRules(s.rules)
    setName(m.name)
    setArea([m.left_mm, m.right_mm])
    setBend(normBend(s.area_bend))
  }, [s, m.name, m.left_mm, m.right_mm])
  const size = sizeOf(sizes, m.size)
  // paper with no lines found: its writing lines can be edited (and are kept once changed)
  const shown = React.useMemo(() => {
    if (rules.length) return rules
    const Hinv = invH(corners, size)
    return (view.lines_px[shot] ?? []).map((l) => l.map((p) => applyH(Hinv, p)))
  }, [rules, corners, size, view.lines_px, shot])

  const run = async (p: Promise<PaperView>) => {
    setBusy(true)
    try {
      onChange(await p)
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    } finally {
      setBusy(false)
    }
  }
  const patch = (p: Parameters<typeof api.updatePaper>[1]) => run(api.updatePaper(m.id, p))
  const saveRules = (r: P2[][] = shown) => void patch({ rules: [shot, r] })

  const addLine = () => {
    const pitch = pitchOf(shown)
    const mid = (l: P2[]) => l[Math.floor(l.length / 2)][1]
    const base = sel !== null ? shown[sel] : [...shown].sort((a, b) => mid(a) - mid(b)).at(-1)
    const fresh: P2[] = base ? base.map(([x, y]) => [x, y + pitch] as P2) : Array.from({ length: 11 }, (_, k) => [(k * size[0]) / 10, 20] as P2)
    const next = [...shown, fresh].sort((a, b) => mid(a) - mid(b))
    setRules(next)
    setSel(next.indexOf(fresh))
    saveRules(next)
  }
  const deleteLine = () => {
    if (sel === null) return
    const next = shown.filter((_, k) => k !== sel)
    setRules(next)
    setSel(null)
    saveRules(next)
  }

  const addShot = async () => {
    const paths = await pickImages(true)
    for (const p of paths) await run(api.addPaperShot(m.id, p))
  }

  const hint = busy ? t('common.working') : mode === 'corners' ? (s.found ? t('papers.cornersHint') : t('papers.noEdges')) : mode === 'lines' ? t('papers.linesHint') : t('papers.areaHint')

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-12 items-center gap-2 border-b border-line bg-surface px-3">
          <Segmented<Mode> label={t('papers.corners')} value={mode} onChange={(md) => { setMode(md); setSel(null) }}
            options={[{ value: 'corners', label: t('papers.corners') }, { value: 'lines', label: t('papers.editLines') }, { value: 'area', label: t('papers.writeArea') }]} />
          <span className="min-w-0 flex-1 truncate text-[0.8rem] text-muted">{hint}</span>
          {mode === 'lines' && (
            <>
              <Button size="sm" onClick={addLine}><Plus />{t('papers.addLine')}</Button>
              <Button size="sm" variant="danger" disabled={sel === null} onClick={deleteLine}><Trash2 />{t('papers.deleteLine')}</Button>
            </>
          )}
          {mode === 'area' && bend.flat().some((v) => Math.abs(v) > 0.05) && (
            <Button size="sm" onClick={() => { const b = flatBend(); setBend(b); void patch({ area_bend: [shot, b] }) }}>{t('papers.straighten')}</Button>
          )}
          <Tip label={t('papers.detect')}>
            <Button size="sm" onClick={() => run(api.calibratePaper(m.id, { shot, corners: null, size: s.found ? m.size : null }))}><ScanLine />{t('papers.detect')}</Button>
          </Tip>
        </div>
        <div className="relative min-h-0 flex-1">
          <ZoomPan width={s.w} height={s.h} fitKey={v} label={m.name}>
            <img src={hw(`shot/${m.id}/${shot}.jpg`, { w: 1800, v })} width={s.w} height={s.h} alt="" draggable={false} className="block select-none" />
            <PaperOverlay view={view} shot={shot} size={size} mode={mode} corners={corners} setCorners={setCorners}
              onCornersDone={() => run(api.calibratePaper(m.id, { shot, corners, size: m.size }))}
              rules={shown} setRules={setRules} onRulesDone={saveRules} sel={sel} setSel={setSel}
              area={area} setArea={setArea} onAreaDone={() => void patch({ left_mm: area[0], right_mm: area[1] })}
              bend={bend} setBend={setBend} onBendDone={() => void patch({ area_bend: [shot, bend] })}
              onStart={(i) => void patch({ start_line: i })}
            />
          </ZoomPan>
          <Legend mode={mode} />
        </div>
        <div className="flex h-24 items-center gap-2 overflow-x-auto border-t border-line bg-surface px-3" aria-label={t('papers.shots')}>
          {m.shots.map((sh, i) => (
            <div key={sh.file} className="group relative">
              <button onClick={() => setShot(i)} aria-pressed={i === shot} className={cn('block overflow-hidden rounded-md border-2', i === shot ? 'border-ink' : 'border-transparent')}>
                <img src={hw(`shot/${m.id}/${i}.jpg`, { w: 160, v: sh.file })} alt={`${t('papers.shots')} ${i + 1}`} className="h-16 w-auto" />
              </button>
              {m.shots.length > 1 && (
                <button className="absolute -right-1.5 -top-1.5 hidden rounded-full border border-line bg-surface p-0.5 text-muted hover:text-redpen group-hover:block" aria-label={t('papers.removeShot')}
                  onClick={() => { setShot(0); void patch({ remove_shot: i }) }}>
                  <X className="size-3" />
                </button>
              )}
            </div>
          ))}
          <Tip label={t('papers.addShot')}>
            <Button variant="quiet" className="h-16 w-16 border border-dashed border-line" onClick={addShot} aria-label={t('papers.addShot')}><ImagePlus className="!size-5" /></Button>
          </Tip>
        </div>
      </div>
      <aside className="w-[18.5rem] shrink-0 overflow-auto border-l border-line bg-surface px-4 py-3" aria-label={t('papers.writeArea')}>
        <Row label={t('papers.name')} htmlFor="p-name" stack>
          <Input id="p-name" value={name} onChange={(e) => setName(e.target.value)} onBlur={() => name !== m.name && patch({ name })} />
        </Row>
        <Row label={t('papers.side')} stack>
          <Segmented<PaperSide> label={t('papers.side')} value={m.side} onChange={(side) => patch({ side })} options={(['left', 'right', 'any'] as PaperSide[]).map((k) => ({ value: k, label: t(`papers.sides.${k}`) }))} className="w-full" />
        </Row>
        <Row label={t('papers.size')} stack>
          <Select label={t('papers.size')} value={m.size} onValueChange={(sz) => run(api.calibratePaper(m.id, { shot, corners, size: sz }))} options={[...Object.keys(sizes).map((k) => ({ value: k, label: `${k} (${sizes[k][0]} × ${sizes[k][1]} mm)` })), ...(sizes[m.size] ? [] : [{ value: m.size, label: t('papers.measured', { w: size[0], h: size[1] }) }])]} />
        </Row>
        <Row label={t('papers.kind')} stack>
          <Segmented<PaperKind> label={t('papers.kind')} value={m.kind} onChange={(kind) => patch({ kind })} options={(['lined', 'grid', 'blank'] as PaperKind[]).map((k) => ({ value: k, label: t(`papers.kinds.${k}`) }))} className="w-full" />
        </Row>
        {m.kind === 'grid' && (
          <Row label={t('papers.step')} hint={t('papers.stepHint')} stack>
            <Segmented label={t('papers.step')} value={String(m.step)} onChange={(st) => run(api.calibratePaper(m.id, { shot, corners, size: m.size, step: Number(st) }))} options={['1', '2', '3'].map((x) => ({ value: x, label: x }))} />
          </Row>
        )}
        <h2 className="mb-1 mt-4 text-[0.8rem] font-semibold">{t('papers.writeArea')}</h2>
        <Row label={t('inspector.startLine')} htmlFor="p-start">
          <NumberField id="p-start" digits={0} min={1} max={Math.max(1, view.lines_px[shot]?.length ?? 1)} value={m.start_line + 1} onCommit={(v) => patch({ start_line: Math.round(v) - 1 })} />
        </Row>
        <Row label={t('inspector.left')} htmlFor="p-left">
          <NumberField id="p-left" min={0} max={area[1] - 20} value={area[0]} onCommit={(v) => patch({ left_mm: v })} />
        </Row>
        <Row label={t('inspector.right')} htmlFor="p-right">
          <NumberField id="p-right" min={0} max={size[0] - area[0] - 20} value={size[0] - area[1]} onCommit={(v) => patch({ right_mm: size[0] - v })} />
        </Row>
        <p className="mt-3 text-[0.8rem] text-muted">
          {t(`papers.kinds.${m.kind}`)}, {t('papers.lines', { n: view.lines_px[shot]?.length ?? 0 })}
        </p>
        <div className="mt-6 flex flex-col gap-2">
          <Button variant="danger" onClick={() => setConfirm(true)}><Trash2 />{t('common.delete')}</Button>
        </div>
        <Dialog open={confirm} onOpenChange={setConfirm} title={t('common.delete')} description={t('common.confirmDelete', { name: m.name })}
          footer={<><Button variant="quiet" onClick={() => setConfirm(false)}>{t('common.cancel')}</Button><Button variant="danger" onClick={async () => { await api.deletePaper(m.id); setConfirm(false); onChange({ ...view, meta: { ...m, id: '' } }) }}>{t('common.delete')}</Button></>}
        >
          <span />
        </Dialog>
      </aside>
    </div>
  )
}

export default function PapersWorkspace() {
  const { t } = useTranslation()
  const paperId = useApp((s) => s.paperId)
  const setPaperId = useApp((s) => s.setPaperId)
  const [papers, setPapers] = React.useState<PaperMeta[]>([])
  const [view, setView] = React.useState<PaperView | null>(null)
  const [adding, setAdding] = React.useState<PaperSide | null>(null)

  const reload = React.useCallback(async (select?: string | null) => {
    const list = await api.listPapers()
    setPapers(list)
    const id = select !== undefined ? select : paperId
    const pick = list.find((p) => p.id === id) ?? list[0]
    if (pick) {
      setPaperId(pick.id)
      setView(await api.getPaper(pick.id))
    } else {
      setView(null)
    }
  }, [paperId, setPaperId])

  React.useEffect(() => {
    void reload()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const add = React.useCallback(async (side: PaperSide, paths?: string[]) => {
    const files = paths ?? (await pickImages(true))
    if (!files.length) return
    setAdding(side)
    let last: string | null = null
    try {
      for (const f of files) {
        const v = await api.addPaper(fileName(f).replace(/\.\w+$/, ''), f, side)
        last = v.meta.id
      }
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    } finally {
      setAdding(null)
    }
    await reload(last)
  }, [reload, t])

  // a photo dropped on the window joins the group of the paper on screen
  const current = view?.meta.id ? view.meta.side : 'any'
  useRequest('addPaper', () => void add(current))
  const over = useFileDrop(true, IMAGE_EXT, (p) => void add(current, p))

  return (
    <div className="flex h-full">
      <aside className="flex w-64 shrink-0 flex-col border-r border-line bg-surface" aria-label={t('papers.title')}>
        <div className="flex h-12 items-center border-b border-line px-3">
          <h1 className="text-[0.95rem] font-semibold">{t('papers.title')}</h1>
        </div>
        <div className="flex-1 overflow-auto p-2">
          {SIDES.map((side) => {
            const list = papers.filter((p) => (p.side ?? 'any') === side)
            return (
              <section key={side} className="mb-4" aria-label={t(`papers.groups.${side}`)}>
                <div className="flex h-8 items-center gap-1 px-1">
                  <h2 className="min-w-0 flex-1 truncate text-[0.84rem] font-semibold">{t(`papers.groups.${side}`)}</h2>
                  <Tip label={t('papers.addTo', { group: t(`papers.groups.${side}`) })}>
                    <Button size="icon-sm" variant="ghost" onClick={() => add(side)} aria-label={t('papers.addTo', { group: t(`papers.groups.${side}`) })} disabled={!!adding}><Plus /></Button>
                  </Tip>
                </div>
                {list.length === 0 && <p className="px-1 text-[0.78rem] text-muted">{adding === side ? t('common.working') : t(`papers.groupHint.${side}`)}</p>}
                <ul className="space-y-1">
                  {list.map((p) => (
                    <li key={p.id}>
                      <button onClick={() => reload(p.id)} aria-current={p.id === view?.meta.id} className={cn('flex w-full items-center gap-3 rounded-lg p-1.5 text-left hover:bg-paper-2', p.id === view?.meta.id && 'bg-ink-soft')}>
                        <img src={hw(`thumb/${p.id}/0.jpg`, { w: 120, v: p.shots[0]?.file, lines: 0 })} alt="" className="h-14 w-11 rounded border border-line object-cover" />
                        <span className="min-w-0">
                          <span className="block truncate font-medium">{p.name}</span>
                          <span className="block text-[0.75rem] text-muted">{t(`papers.kinds.${p.kind}`)}, {p.size}{p.shots.length > 1 ? ` · ${t('papers.photos', { n: p.shots.length })}` : ''}</span>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            )
          })}
          <p className="px-1 text-[0.78rem] text-muted">{t('papers.notebookHint')}</p>
        </div>
      </aside>
      <div className={cn('flex min-w-0 flex-1', over && 'ring-4 ring-inset ring-ink/40')}>
        {view && view.meta.id ? (
          <PaperEditor key={view.meta.id} view={view} onChange={(v) => (v.meta.id ? (setView(v), setPapers((ps) => ps.map((p) => (p.id === v.meta.id ? v.meta : p)))) : void reload(null))} />
        ) : (
          <EmptyState art={<NotebookArt />} title={t('papers.empty')} body={t('papers.emptyBody')}
            action={
              <div className="flex flex-wrap justify-center gap-2">
                {SIDES.map((side) => (
                  <Button key={side} variant={side === 'any' ? 'secondary' : 'primary'} size="lg" onClick={() => add(side)} disabled={!!adding}><ImagePlus />{t(`papers.sides.${side}`)}</Button>
                ))}
              </div>
            } />
        )}
      </div>
      {adding && <span className="sr-only" role="status">{t('common.working')}</span>}
    </div>
  )
}
