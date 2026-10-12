import { Check, ChevronDown, ChevronLeft, ChevronRight, Circle, FolderOpen, ImagePlus, Plus, Trash2, TriangleAlert, X } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import { Dialog } from '@/components/ui/overlay'
import { api, SHARP_PX_MM, type GuidePage, type GuideView, type HandView, type Lang, type Report } from '@/lib/api'
import { IMAGE_EXT, pickFolder, pickImages, useFileDrop } from '@/lib/files'
import { cn } from '@/lib/utils'
import { runJob } from '@/state/jobs'
import { usePref } from '@/state/prefs'

import { FolderDialog } from './FolderDialog'

export function useLangName() {
  const { t } = useTranslation()
  return React.useCallback((l: Lang) => (l === 'uz_latn' || l === 'uz_cyrl' ? `${t(`hands.langs.${l}`)} (${t(`hands.scripts.${l}`)})` : t(`hands.langs.${l}`)), [t])
}

function PageRow({ p, name, current, onSelect }: { p: GuidePage; name: string; current: boolean; onSelect: () => void }) {
  return (
    <li>
      <button type="button" onClick={onSelect} aria-current={current ? 'true' : undefined}
        className={cn('flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[0.84rem] hover:bg-paper-2', current && 'bg-paper-2 font-medium')}>
        {p.done ? <Check className="size-4 shrink-0 text-ok" /> : <Circle className="size-4 shrink-0 text-line" />}
        <span className="min-w-0 flex-1 truncate">{name}</span>
        {p.done && <span className="text-[0.75rem] tabular-nums text-muted">{p.done.letters}</span>}
      </button>
    </li>
  )
}

function Section({ title, count, action, children }: { title: string; count?: string; action?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="mb-5">
      <div className="mb-1 flex h-7 items-center gap-1 px-1">
        <h2 className="min-w-0 flex-1 truncate text-[0.85rem] font-semibold">{title}</h2>
        {count && <span className="text-[0.78rem] tabular-nums text-muted">{count}</span>}
        {action}
      </div>
      {children}
    </section>
  )
}

type Removing = { id: number; name: string }

/**
 * Samples to write by hand on any paper. Every built-in page is required (built from real
 * text so nearly every join has a written version); then pages of what did not come through,
 * and the person's own writing: photos with their text, one by one or a folder at a time.
 */
export function Guide({ hand, onChanged, focus, onOwnPage }: { hand: HandView; onChanged: () => void; focus: number; onOwnPage: () => void }) {
  const { t } = useTranslation()
  const langName = useLangName()
  const [view, setView] = React.useState<GuideView | null>(null)
  const [sel, setSel] = React.useState<string | null>(null)
  const [open, setOpen] = usePref<Record<string, boolean>>('guide.open', {})
  const [busy, setBusy] = React.useState(false)
  const [result, setResult] = React.useState<Report | null>(null)
  const [removing, setRemoving] = React.useState<Removing | null>(null)
  const [folder, setFolder] = React.useState<string | null>(null)
  const id = hand.meta.id

  const load = React.useCallback(async () => setView(await api.guidePages(id)), [id])
  React.useEffect(() => {
    void load()
  }, [load, hand.meta.imports.length])

  // pages in the order they are written
  const pages: GuidePage[] = React.useMemo(() => (view ? [...view.required.flatMap((g) => g.pages), ...view.missing, ...view.fill] : []), [view])
  const page = pages.find((p) => p.id === sel) ?? pages.find((p) => !p.done) ?? pages[0]
  // "write missing letters" from the characters tab: jump once to the page of only those
  const focused = React.useRef(0)
  React.useEffect(() => {
    if (focus === focused.current || !pages.length) return
    focused.current = focus
    setSel((pages.find((p) => p.kind === 'fill' && !p.done) ?? pages.find((p) => p.kind === 'extra' && !p.done) ?? pages.find((p) => !p.done))?.id ?? null)
  }, [focus, pages])

  const select = (s: string) => {
    setSel(s)
    setResult(null)
  }
  const idx = page ? pages.findIndex((p) => p.id === page.id) : -1
  const go = (k: number) => pages[idx + k] && select(pages[idx + k].id)

  // a page that did not fit on one sheet: all its photos, in order
  const addPhoto = React.useCallback(async (paths: string[]) => {
    if (!paths.length || !page || busy) return
    // stay on this page once it is done
    setSel(page.id)
    setBusy(true)
    setResult(null)
    try {
      const r = await runJob<Report>(() => api.importGuidePage(id, page.id, page.lines, page.print, [...paths].sort((a, b) => a.localeCompare(b, undefined, { numeric: true }))))
      setResult(r)
      if (!r.ok) toast.error(r.error ?? t('errors.generic', { msg: '' }))
      await load()
      onChanged()
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    } finally {
      setBusy(false)
    }
  }, [page, busy, id, load, onChanged, t])
  const over = useFileDrop(!!page && !busy, IMAGE_EXT, (p) => void addPhoto(p))

  const remove = async () => {
    if (!removing) return
    await api.removeImport(id, removing.id)
    setRemoving(null)
    await load()
    onChanged()
  }

  const name = (p: GuidePage) =>
    p.kind === 'joined' ? t('hands.guide.joined', { n: p.n })
      : p.kind === 'print' ? t('hands.guide.print')
        : p.kind === 'separate' ? t('hands.guide.separate')
          : p.kind === 'math' ? t('hands.guide.formulas')
            : p.kind === 'fill' ? t('hands.guide.fill', { n: p.n })
              : t('hands.guide.extra', { n: p.n })
  const title = (p: GuidePage) => (p.kind === 'math' ? t('hands.guide.math') : p.lang ? `${langName(p.lang)} · ${name(p)}` : name(p))
  const required = view?.required.flatMap((g) => g.pages) ?? []
  const reqDone = required.filter((p) => p.done).length

  return (
    <div className="flex h-full min-h-0">
      <aside className="w-68 shrink-0 overflow-auto border-r border-line bg-surface px-3 py-3" aria-label={t('hands.guide.title')}>
        <Section title={t('hands.guide.required')} count={`${reqDone}/${required.length}`}>
          <div className="space-y-0.5">
            {view?.required.map((g) => {
              const gd = g.pages.filter((p) => p.done).length
              const expanded = open[g.lang] ?? g.pages.some((p) => p.id === page?.id)
              return (
                <div key={g.lang}>
                  <button type="button" aria-expanded={expanded} onClick={() => setOpen({ ...open, [g.lang]: !expanded })}
                    className="flex w-full items-center gap-1.5 rounded-md px-1 py-1.5 text-left text-[0.84rem] hover:bg-paper-2">
                    <ChevronDown className={cn('size-4 shrink-0 text-muted transition-transform', !expanded && '-rotate-90')} />
                    <span className="min-w-0 flex-1 truncate">{g.lang === 'math' ? t('hands.guide.math') : langName(g.lang)}</span>
                    {gd === g.pages.length ? <Check className="size-4 text-ok" /> : <span className="text-[0.75rem] tabular-nums text-muted">{gd}/{g.pages.length}</span>}
                  </button>
                  {expanded && (
                    <ol className="ml-3 space-y-0.5">
                      {g.pages.map((p) => <PageRow key={p.id} p={p} name={name(p)} current={p.id === page?.id} onSelect={() => select(p.id)} />)}
                    </ol>
                  )}
                </div>
              )
            })}
            {view && view.missing.length > 0 && (
              <ol className="space-y-0.5 pt-1">
                {view.missing.map((p) => <PageRow key={p.id} p={p} name={name(p)} current={p.id === page?.id} onSelect={() => select(p.id)} />)}
              </ol>
            )}
          </div>
          {required.length > 0 && reqDone === required.length && (
            <p className="mt-1 flex items-center gap-1.5 px-2 text-[0.8rem] text-ok"><Check className="size-3.5" />{t('hands.guide.ready')}</p>
          )}
        </Section>

        {view && view.fill.length > 0 && (
          <Section title={t('hands.guide.fillTitle')} count={String(view.fill.filter((p) => !p.done).length || '')}>
            <ol className="space-y-0.5">
              {view.fill.map((p) => <PageRow key={p.id} p={p} name={name(p)} current={p.id === page?.id} onSelect={() => select(p.id)} />)}
            </ol>
          </Section>
        )}

        <Section title={t('hands.guide.own')}>
          <div className="mb-1.5 flex gap-1.5 px-1">
            <Button size="sm" onClick={onOwnPage}><Plus />{t('hands.guide.ownAdd')}</Button>
            <Button size="sm" onClick={async () => setFolder(await pickFolder())}><FolderOpen />{t('hands.guide.ownFolder')}</Button>
          </div>
          {view && view.own.length === 0 && <p className="px-2 text-[0.8rem] text-muted">{t('hands.guide.ownHint')}</p>}
          <ol className="space-y-0.5">
            {view?.own.map((o) => (
              <li key={o.import} className="group flex items-center gap-2 rounded-md py-0.5 pl-2 text-[0.82rem]">
                <Check className="size-4 shrink-0 text-ok" />
                <span className="min-w-0 flex-1 truncate text-graphite" title={o.name}>{o.name || '—'}</span>
                {o.small && <TriangleAlert className="size-3.5 shrink-0 text-redpen" aria-label={t('hands.photo.small')} />}
                <span className="text-[0.75rem] tabular-nums text-muted">{o.letters}</span>
                <Button variant="quiet" size="icon-sm" aria-label={`${t('hands.guide.remove')}: ${o.name}`} className="opacity-60 group-hover:opacity-100 focus-visible:opacity-100"
                  onClick={() => setRemoving({ id: o.import, name: o.name || '—' })}><X /></Button>
              </li>
            ))}
          </ol>
        </Section>
      </aside>

      <section className="min-w-0 flex-1 overflow-auto bg-paper-2" aria-label={page ? title(page) : t('hands.guide.title')}>
        {page && (
          <div className="mx-auto max-w-3xl space-y-4 px-6 pt-5">
            <header className="flex items-center gap-3">
              <h2 className="min-w-0 flex-1 text-[1.15rem] font-semibold">{title(page)}</h2>
              <Button variant="ghost" size="icon" onClick={() => go(-1)} disabled={idx <= 0} aria-label={t('common.back')}><ChevronLeft /></Button>
              <Button variant="ghost" size="icon" onClick={() => go(1)} disabled={idx >= pages.length - 1} aria-label={t('common.next')}><ChevronRight /></Button>
            </header>

            <p className="rounded-lg border-l-4 border-ink bg-surface px-4 py-2.5 text-[0.92rem] font-medium leading-snug">{t(`hands.guide.do_${page.kind}`)}</p>

            <article className="rounded-xl border border-line bg-surface px-8 py-5 shadow-sm" lang={page.lang?.slice(0, 2)}>
              {page.lines.map((l, i) => (
                <p key={i} className="flex items-baseline border-b border-dashed border-grid py-1.5 text-[1.3rem] leading-relaxed text-graphite">
                  <span className="min-w-0 flex-1">
                    {page.kind === 'separate' || page.kind === 'fill'
                      ? l.split(' ').map((c, j) => <span key={j} className="mr-[1.1em] inline-block">{c}</span>)
                      : l}
                  </span>
                  {/* a page of several kinds of letters: the lines to write in print */}
                  {page.kind === 'fill' && page.print[i] && <span className="ml-2 shrink-0 rounded bg-ink-soft px-1.5 py-0.5 text-[0.75rem] font-medium text-ink">{t('hands.guide.printTag')}</span>}
                </p>
              ))}
            </article>

            <div className="sticky bottom-0 space-y-1.5 bg-paper-2/95 pb-4 pt-2 backdrop-blur-sm">
              {busy ? (
                <p role="status" className="rounded-xl border border-line bg-surface px-4 py-4 text-center text-muted">{t('hands.guide.reading')}</p>
              ) : page.done && !result ? (
                <div className="flex flex-wrap items-center gap-3 rounded-xl border border-line bg-surface px-4 py-3" role="status">
                  <Check className="size-5 text-ok" />
                  <span className="flex-1">
                    {t('hands.guide.done', { n: page.done.letters })}
                    {page.done.unmatched.length > 0 && <span className="block text-[0.8rem] text-muted">{t('hands.photo.unmatched', { words: page.done.unmatched.join(', ') })}</span>}
                    {page.done.small && <span className="block text-[0.8rem] text-redpen">{t('hands.photo.small')}</span>}
                  </span>
                  <Button onClick={async () => addPhoto(await pickImages(true))}><ImagePlus />{t('hands.guide.retake')}</Button>
                  {idx < pages.length - 1 && <Button variant="primary" onClick={() => go(1)}>{t('hands.guide.next')}<ChevronRight /></Button>}
                </div>
              ) : result ? (
                <div className={cn('flex flex-wrap items-center gap-3 rounded-xl border border-line bg-surface px-4 py-3', !result.ok && 'border-redpen/40')} role="status">
                  {result.ok ? <Check className="size-5 text-ok" /> : <TriangleAlert className="size-5 text-redpen" />}
                  <span className="flex-1">
                    {result.ok ? t('hands.guide.done', { n: result.glyphs }) : result.error}
                    {result.ok && result.px_mm > 0 && result.px_mm < SHARP_PX_MM && <span className="block text-[0.8rem] text-redpen">{t('hands.photo.small')}</span>}
                  </span>
                  {idx < pages.length - 1 && <Button variant="primary" onClick={() => go(1)}>{t('hands.guide.next')}<ChevronRight /></Button>}
                </div>
              ) : (
                <button onClick={async () => addPhoto(await pickImages(true))}
                  className={cn('flex w-full items-center justify-center gap-2 rounded-xl border-2 border-dashed border-ink/40 bg-surface px-4 py-4 text-ink hover:border-ink hover:bg-ink-soft/40', over && 'border-ink bg-ink-soft/50')}>
                  <ImagePlus className="size-5" />
                  {t('hands.guide.addPhoto')}
                </button>
              )}
              <p className="text-[0.78rem] text-muted">{t('hands.guide.photo_tip')}</p>
            </div>
          </div>
        )}
      </section>

      <Dialog open={!!removing} onOpenChange={(o) => !o && setRemoving(null)} title={t('hands.guide.remove')} description={t('hands.guide.removeAsk', { name: removing?.name ?? '' })}
        footer={<><Button variant="quiet" onClick={() => setRemoving(null)}>{t('common.cancel')}</Button><Button variant="danger" onClick={() => void remove()}><Trash2 />{t('hands.guide.remove')}</Button></>}>
        {null}
      </Dialog>
      <FolderDialog hand={id} dir={folder} onClose={() => setFolder(null)} onImported={() => { void load(); onChanged() }} />
    </div>
  )
}
