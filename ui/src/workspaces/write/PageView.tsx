import { Maximize2 } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'

import { ZoomPan } from '@/components/ZoomPan'
import { Button } from '@/components/ui/button'
import { Dialog, Tip } from '@/components/ui/overlay'
import { hw, type PageSheet } from '@/lib/api'

export interface GalleryHandle {
  /** Scroll page i into view. */
  show: (i: number) => void
}

/** True while `el` is on screen or close to it (pages further away are not rendered). */
function useNear(el: React.RefObject<HTMLElement | null>) {
  const [near, setNear] = React.useState(false)
  React.useEffect(() => {
    const node = el.current
    if (!node) return
    const io = new IntersectionObserver(([e]) => setNear(e.isIntersecting), { rootMargin: '800px 0px' })
    io.observe(node)
    return () => io.disconnect()
  }, [el])
  return near
}

/** The page's photo: the last render stays on screen until the new one has loaded. */
function PageImage({ src, near }: { src: string; near: boolean }) {
  const { t } = useTranslation()
  const [shown, setShown] = React.useState<string | null>(null)
  const [busy, setBusy] = React.useState(false)
  React.useEffect(() => {
    if (!near || src === shown) return
    let alive = true
    setBusy(true)
    const img = new Image()
    img.onload = () => {
      if (!alive) return
      setShown(src)
      setBusy(false)
    }
    img.onerror = () => alive && setBusy(false)
    img.src = src
    return () => {
      alive = false
    }
  }, [src, near, shown])
  return (
    <>
      {shown && <img src={shown} alt="" draggable={false} className="absolute inset-0 size-full select-none" />}
      {(busy || !shown) && <span role="status" className="absolute left-2 top-2 rounded-md bg-surface/90 px-2 py-1 text-[0.75rem] text-muted shadow-sm">{t('write.rendering')}</span>}
    </>
  )
}

/** One page of the gallery: caption above, the page, a button to see it large. */
function Page({ i, total, caption, ratio, onOpen, children, register }: {
  i: number
  total: number
  caption?: string
  ratio: number
  onOpen: () => void
  children: (near: boolean) => React.ReactNode
  register: (i: number, el: HTMLElement | null) => void
}) {
  const { t } = useTranslation()
  const ref = React.useRef<HTMLElement | null>(null)
  const near = useNear(ref)
  return (
    <figure ref={(el) => { ref.current = el; register(i, el) }} data-page={i} className="group scroll-mt-4">
      <figcaption className="mb-1.5 flex items-baseline gap-3 text-[0.78rem] text-muted">
        <span className="font-medium text-graphite">{t('common.page', { n: i + 1, total })}</span>
        {caption && <span className="min-w-0 flex-1 truncate">{caption}</span>}
      </figcaption>
      <div className="relative w-full overflow-hidden bg-surface shadow-[0_1px_4px_rgb(0_0_0/0.18)]" style={{ aspectRatio: ratio }}>
        {children(near)}
        <Tip label={t('write.openLarge')}>
          <Button size="icon-sm" variant="secondary" onClick={onOpen} aria-label={t('write.openLarge')}
            className="absolute right-2 top-2 opacity-0 shadow-sm transition-opacity focus-visible:opacity-100 group-hover:opacity-100">
            <Maximize2 />
          </Button>
        </Tip>
      </div>
    </figure>
  )
}

/** Pages one under another, scrolled through like a gallery; reports the page in view. */
function Pages({ count, onPage, handle, children }: {
  count: number
  onPage: (i: number) => void
  handle: React.Ref<GalleryHandle>
  children: (register: (i: number, el: HTMLElement | null) => void) => React.ReactNode
}) {
  const box = React.useRef<HTMLDivElement>(null)
  const els = React.useRef<(HTMLElement | null)[]>([])
  const register = React.useCallback((i: number, el: HTMLElement | null) => {
    els.current[i] = el
  }, [])
  React.useImperativeHandle(handle, () => ({ show: (i) => els.current[i]?.scrollIntoView({ behavior: 'smooth', block: 'start' }) }), [])
  // the page in view: the last one whose top has passed a third of the way down
  const onScroll = React.useCallback(() => {
    const b = box.current
    if (!b) return
    const line = b.getBoundingClientRect().top + b.clientHeight / 3
    let cur = 0
    els.current.slice(0, count).forEach((el, i) => {
      if (el && el.getBoundingClientRect().top <= line) cur = i
    })
    onPage(cur)
  }, [count, onPage])
  React.useEffect(onScroll, [onScroll])
  return (
    <div ref={box} onScroll={onScroll} className="kletka h-full overflow-auto px-6 py-5">
      <div className="mx-auto flex max-w-[46rem] flex-col gap-7 pb-16">{children(register)}</div>
    </div>
  )
}

/** Every page on its paper photo. */
export const PhotoGallery = React.forwardRef<GalleryHandle, {
  project: string
  paper: string
  sheets: PageSheet[]
  pages: number
  version: number
  preview: { random: boolean; seed: number }
  onPage: (i: number) => void
}>(function PhotoGallery({ project, paper, sheets, pages, version, preview, onPage }, ref) {
  const { t } = useTranslation()
  const [open, setOpen] = React.useState<number | null>(null)
  const src = (i: number, w: number) => hw(`preview/${project}/${paper}/${i}.jpg`, { w, v: version, shot: sheets[i]?.shot ?? 0, random: preview.random ? 1 : 0, seed: preview.seed + i })
  const caption = (i: number) => {
    const s = sheets[i]
    return s ? (s.side === 'any' ? s.name : `${t(`papers.sides.${s.side}`)} · ${s.name}`) : undefined
  }
  const big = open !== null ? sheets[open] : undefined
  return (
    <>
      <Pages count={pages} onPage={onPage} handle={ref}>
        {(register) => Array.from({ length: pages }, (_, i) => (
          <Page key={i} i={i} total={pages} caption={caption(i)} ratio={sheets[i] ? sheets[i].w / sheets[i].h : 0.75} onOpen={() => setOpen(i)} register={register}>
            {(near) => <PageImage src={src(i, 1200)} near={near} />}
          </Page>
        ))}
      </Pages>
      <Dialog open={open !== null} onOpenChange={(o) => !o && setOpen(null)} title={open !== null ? t('common.page', { n: open + 1, total: pages }) : ''} description={open !== null ? caption(open) : undefined} width="max-w-[92vw]">
        {open !== null && big && (
          <div className="h-[76vh]">
            <ZoomPan width={big.w} height={big.h} fitKey={`${open}-${version}`} label={t('write.view.photo')}>
              <img src={src(open, Math.max(big.w, 1600))} width={big.w} height={big.h} alt="" draggable={false} className="block select-none" />
            </ZoomPan>
          </div>
        )}
      </Dialog>
    </>
  )
})

/** One clean page as SVG: click any letter to swap it for another version. */
function CleanSvg({ project, page, version, near, onSwap }: { project: string; page: number; version: number; near: boolean; onSwap: (w: number, c: number) => void }) {
  const { t } = useTranslation()
  const [svg, setSvg] = React.useState('')
  const [loaded, setLoaded] = React.useState<number | null>(null)
  React.useEffect(() => {
    if (!near || loaded === version) return
    let alive = true
    void fetch(hw(`svg/${project}/${page}.svg`, { v: version }))
      .then((r) => r.text())
      .then((s) => {
        if (!alive) return
        setSvg(s)
        setLoaded(version)
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [project, page, version, near, loaded])
  return (
    <div
      className="absolute inset-0 bg-white [&_svg]:size-full [&_use]:cursor-pointer [&_use:hover]:opacity-60"
      title={t('write.swapHint')}
      onClick={(e) => {
        const u = (e.target as Element).closest('use')
        if (u) onSwap(Number(u.getAttribute('data-w')), Number(u.getAttribute('data-c')))
      }}
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  )
}

/** Every page as a clean sheet (210 x 297 mm). */
export const CleanGallery = React.forwardRef<GalleryHandle, {
  project: string
  pages: number
  version: number
  onSwap: (w: number, c: number) => void
  onPage: (i: number) => void
}>(function CleanGallery({ project, pages, version, onSwap, onPage }, ref) {
  const { t } = useTranslation()
  const [open, setOpen] = React.useState<number | null>(null)
  return (
    <>
      <Pages count={pages} onPage={onPage} handle={ref}>
        {(register) => Array.from({ length: pages }, (_, i) => (
          <Page key={i} i={i} total={pages} ratio={210 / 297} onOpen={() => setOpen(i)} register={register}>
            {(near) => <CleanSvg project={project} page={i} version={version} near={near} onSwap={onSwap} />}
          </Page>
        ))}
      </Pages>
      <Dialog open={open !== null} onOpenChange={(o) => !o && setOpen(null)} title={open !== null ? t('common.page', { n: open + 1, total: pages }) : ''} width="max-w-[92vw]">
        {open !== null && (
          <div className="h-[76vh]">
            <ZoomPan width={840} height={1188} fitKey={`${open}-${version}`} label={t('write.view.svg')}>
              <div className="relative h-[1188px] w-[840px]">
                <CleanSvg project={project} page={open} version={version} near onSwap={onSwap} />
              </div>
            </ZoomPan>
          </div>
        )}
      </Dialog>
    </>
  )
})
