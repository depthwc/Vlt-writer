import { Maximize, Minus, Plus } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Tip } from '@/components/ui/overlay'
import { cn, clamp } from '@/lib/utils'

interface View {
  s: number
  x: number
  y: number
}

export interface ZoomCtx {
  scale: number
  /** client (screen) point -> content pixel point */
  toContent: (cx: number, cy: number) => [number, number]
}

const Ctx = React.createContext<ZoomCtx>({ scale: 1, toContent: (x, y) => [x, y] })
export const useZoom = () => React.useContext(Ctx)

/**
 * Pan/zoom surface on squared notebook paper. Content is laid out in its own pixel space
 * (width x height); children can read the scale to keep handles a constant size on screen.
 */
export function ZoomPan({ width, height, children, fitKey, className, label, padding = 32, toolbar = true }: {
  width: number
  height: number
  children: React.ReactNode
  fitKey?: string | number
  className?: string
  label: string
  padding?: number
  toolbar?: boolean
}) {
  const { t } = useTranslation()
  const box = React.useRef<HTMLDivElement>(null)
  const [view, setView] = React.useState<View>({ s: 1, x: 0, y: 0 })
  const drag = React.useRef<{ x: number; y: number; vx: number; vy: number } | null>(null)

  const fit = React.useCallback(() => {
    const el = box.current
    if (!el || !width || !height) return
    const r = el.getBoundingClientRect()
    const s = Math.min((r.width - 2 * padding) / width, (r.height - 2 * padding) / height)
    const ss = s > 0 ? s : 1
    setView({ s: ss, x: (r.width - width * ss) / 2, y: (r.height - height * ss) / 2 })
  }, [width, height, padding])

  React.useLayoutEffect(() => {
    fit()
  }, [fit, fitKey])

  React.useEffect(() => {
    const el = box.current
    if (!el) return
    const ro = new ResizeObserver(() => fit())
    ro.observe(el)
    return () => ro.disconnect()
  }, [fit])

  const zoomAt = React.useCallback((factor: number, cx?: number, cy?: number) => {
    const el = box.current
    if (!el) return
    const r = el.getBoundingClientRect()
    const px = cx === undefined ? r.width / 2 : cx - r.left
    const py = cy === undefined ? r.height / 2 : cy - r.top
    setView((v) => {
      const s = clamp(v.s * factor, 0.05, 12)
      const k = s / v.s
      return { s, x: px - (px - v.x) * k, y: py - (py - v.y) * k }
    })
  }, [])

  React.useEffect(() => {
    const el = box.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      zoomAt(Math.exp(-e.deltaY * 0.0015), e.clientX, e.clientY)
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [zoomAt])

  const ctx = React.useMemo<ZoomCtx>(() => ({
    scale: view.s,
    toContent: (cx, cy) => {
      const r = box.current!.getBoundingClientRect()
      return [(cx - r.left - view.x) / view.s, (cy - r.top - view.y) / view.s]
    },
  }), [view])

  return (
    <div
      ref={box}
      role="region"
      aria-label={label}
      tabIndex={0}
      className={cn('kletka relative h-full w-full touch-none overflow-hidden outline-none', className)}
      onPointerDown={(e) => {
        if (e.button !== 0 && e.button !== 1) return
        if ((e.target as HTMLElement).closest('[data-handle]')) return
        drag.current = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y }
        ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
      }}
      onPointerMove={(e) => {
        const d = drag.current
        if (!d) return
        setView((v) => ({ ...v, x: d.vx + e.clientX - d.x, y: d.vy + e.clientY - d.y }))
      }}
      onPointerUp={() => (drag.current = null)}
      onKeyDown={(e) => {
        if (e.target !== e.currentTarget) return
        if (e.key === '+' || e.key === '=') zoomAt(1.25)
        else if (e.key === '-') zoomAt(0.8)
        else if (e.key === '0') fit()
      }}
    >
      <div className="absolute left-0 top-0 origin-top-left" style={{ width, height, transform: `translate(${view.x}px, ${view.y}px) scale(${view.s})` }}>
        <Ctx.Provider value={ctx}>{children}</Ctx.Provider>
      </div>
      {toolbar && (
        <div className="absolute bottom-3 right-3 flex items-center gap-0.5 rounded-lg border border-line bg-surface/95 p-0.5 shadow-sm">
          <Tip label={t('common.zoomOut')} shortcut="−">
            <Button variant="ghost" size="icon-sm" onClick={() => zoomAt(0.8)} aria-label={t('common.zoomOut')}><Minus /></Button>
          </Tip>
          <span className="w-11 text-center text-[0.75rem] tabular-nums text-muted">{Math.round(view.s * 100)}%</span>
          <Tip label={t('common.zoomIn')} shortcut="+">
            <Button variant="ghost" size="icon-sm" onClick={() => zoomAt(1.25)} aria-label={t('common.zoomIn')}><Plus /></Button>
          </Tip>
          <Tip label={t('common.fit')} shortcut="0">
            <Button variant="ghost" size="icon-sm" onClick={fit} aria-label={t('common.fit')}><Maximize /></Button>
          </Tip>
        </div>
      )}
    </div>
  )
}

/** A draggable, keyboard-movable handle in content space (constant screen size). */
export function Handle({ x, y, onMove, onDone, label, selected, onSelect, color = 'var(--ink)', shape = 'circle' }: {
  x: number
  y: number
  onMove: (x: number, y: number) => void
  onDone?: () => void
  label: string
  selected?: boolean
  onSelect?: () => void
  color?: string
  shape?: 'circle' | 'square'
}) {
  const { scale, toContent } = useZoom()
  const r = 7 / scale
  const dragging = React.useRef(false)
  return (
    <g
      data-handle
      role="slider"
      aria-label={label}
      aria-valuetext={`${Math.round(x)}, ${Math.round(y)}`}
      tabIndex={0}
      className="cursor-grab outline-none focus-visible:[&>*:first-child]:stroke-[3]"
      onPointerDown={(e) => {
        e.stopPropagation()
        dragging.current = true
        onSelect?.()
        ;(e.currentTarget as Element).setPointerCapture(e.pointerId)
      }}
      onPointerMove={(e) => {
        if (!dragging.current) return
        const [cx, cy] = toContent(e.clientX, e.clientY)
        onMove(cx, cy)
      }}
      onPointerUp={() => {
        if (dragging.current) onDone?.()
        dragging.current = false
      }}
      onKeyDown={(e) => {
        const step = (e.shiftKey ? 10 : 1) / scale
        const d: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }
        if (d[e.key]) {
          e.preventDefault()
          onMove(x + d[e.key][0], y + d[e.key][1])
        }
      }}
      onKeyUp={(e) => e.key.startsWith('Arrow') && onDone?.()}
      onFocus={onSelect}
    >
      {shape === 'circle' ? (
        <circle cx={x} cy={y} r={r} fill="var(--surface)" stroke={color} strokeWidth={(selected ? 3 : 2) / scale} />
      ) : (
        <rect x={x - r} y={y - r} width={2 * r} height={2 * r} rx={2 / scale} fill="var(--surface)" stroke={color} strokeWidth={(selected ? 3 : 2) / scale} />
      )}
      <circle cx={x} cy={y} r={r * 2.2} fill="transparent" />
    </g>
  )
}
