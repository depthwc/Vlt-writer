import { CaseSensitive, ChevronDown, FileInput, FileOutput, ListChecks, NotebookPen, Pencil, Plus, Search, Trash2, X } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { EmptyState, NotebookArt } from '@/components/Shell'
import { Button } from '@/components/ui/button'
import { Input, Progress, Segmented, Select, Slider } from '@/components/ui/controls'
import { Dialog, Menu, MenuContent, MenuItem, MenuSeparator, MenuTrigger } from '@/components/ui/overlay'
import { api, type HandView, type Lang, type Summary } from '@/lib/api'
import { pickFile, saveAs } from '@/lib/files'
import { useApp } from '@/state/app'
import { useRequest } from '@/state/bus'
import { usePref } from '@/state/prefs'

import { RepairPanel } from './Repair'
import { Guide } from './Guide'
import { LetterTree, letterGroups, type Filter } from './LetterTree'
import { PhotoDialog } from './PhotoDialog'

function NameDialog({ open, onOpenChange, title, initial, onSubmit }: { open: boolean; onOpenChange: (v: boolean) => void; title: string; initial: string; onSubmit: (n: string) => void }) {
  const { t } = useTranslation()
  const [name, setName] = React.useState(initial)
  React.useEffect(() => setName(initial), [initial, open])
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title={title} width="max-w-sm"
      footer={<><Button variant="quiet" onClick={() => onOpenChange(false)}>{t('common.cancel')}</Button><Button variant="primary" disabled={!name.trim()} onClick={() => onSubmit(name.trim())}>{t('common.save')}</Button></>}
    >
      <form onSubmit={(e) => { e.preventDefault(); if (name.trim()) onSubmit(name.trim()) }} className="pb-2">
        <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={t('hands.namePlaceholder')} aria-label={t('hands.name')} />
      </form>
    </Dialog>
  )
}

export default function HandsWorkspace() {
  const { t, i18n } = useTranslation()
  const handId = useApp((s) => s.handId)
  const setHandId = useApp((s) => s.setHandId)
  const [hands, setHands] = React.useState<Summary[]>([])
  const [view, setView] = React.useState<HandView | null>(null)
  const [sel, setSel] = React.useState<string | null>(null)
  const [filter, setFilter] = usePref<Filter>('hands.filter', 'all')
  const [find, setFind] = React.useState('')
  // several versions picked at once (to delete them)
  const [picking, setPicking] = React.useState(false)
  const [picked, setPicked] = React.useState<Set<string>>(new Set())
  const [askDelete, setAskDelete] = React.useState(false)
  // pages to write (the start, and later more pages) or the letters found on them
  const [mode, setMode] = usePref<'pages' | 'letters'>('hands.mode', 'letters')
  const [focus, setFocus] = React.useState(0)
  const [photo, setPhoto] = React.useState(false)
  const [naming, setNaming] = React.useState<'new' | 'rename' | null>(null)
  const [confirm, setConfirm] = React.useState(false)
  const [v, setV] = React.useState(0)
  const grid = React.useRef<HTMLDivElement>(null)

  const load = React.useCallback(async (id?: string | null) => {
    const list = await api.listHands()
    setHands(list)
    const pick = list.find((h) => h.id === (id ?? handId)) ?? list[0]
    if (pick) {
      setHandId(pick.id)
      const hv = await api.getHand(pick.id)
      setView(hv)
      // nothing written yet: the pages to write
      if (!hv.glyphs.length) setMode('pages')
    } else {
      setView(null)
    }
  }, [handId, setHandId])

  React.useEffect(() => {
    void load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // characters a document needs: a page of them to write, ready in the photo dialog
  const toWrite = useApp((s) => s.toWrite)
  const setToWrite = useApp((s) => s.setToWrite)
  const [given, setGiven] = React.useState<string | null>(null)
  const [givenPrint, setGivenPrint] = React.useState(false)
  React.useEffect(() => {
    if (!toWrite || !view) return
    setGiven([...toWrite.chars].map((c) => `${c} ${c} ${c}`).join('\n'))
    setGivenPrint(toWrite.print)
    setToWrite(null)
    setMode('pages')
    setPhoto(true)
  }, [toWrite, view, setToWrite])

  useRequest('printSheet', () => setMode('pages'))
  useRequest('printRefill', () => { setMode('pages'); setFocus((f) => f + 1) })

  const refresh = React.useCallback(async () => {
    if (!view) return
    setView(await api.getHand(view.meta.id))
    // the count in the handwriting list too
    setHands(await api.listHands())
    setV((x) => x + 1)
  }, [view])

  // every language is written; the interface language's comes first
  const langs = (): Lang[] => {
    const main: Lang = i18n.language.startsWith('ru') ? 'ru' : 'uz_latn'
    return [main, ...(['uz_latn', 'ru', 'uz_cyrl', 'en', 'math'] as Lang[]).filter((l) => l !== main)]
  }
  const create = async (name: string) => {
    const id = await api.createHand(name)
    await api.setHandLangs(id, langs())
    setNaming(null)
    await load(id)
    setMode('pages')
  }
  // first start: a handwriting with the interface language, straight to the pages to write
  const start = async () => {
    const id = await api.createHand(t('hands.defaultName'))
    await api.setHandLangs(id, langs())
    await load(id)
    setMode('pages')
  }

  const groups = React.useMemo(() => (view ? letterGroups(view, filter, find) : []), [view, filter, find])
  const shown = React.useMemo(() => groups.flatMap((g) => g.letters.flatMap((l) => l.glyphs.map((x) => x.id))), [groups])
  const pick = (ids: string[], on: boolean) => {
    setPicking(true)
    setPicked((s) => {
      const n = new Set(s)
      ids.forEach((id) => (on ? n.add(id) : n.delete(id)))
      return n
    })
  }
  const stopPicking = () => { setPicking(false); setPicked(new Set()) }
  // the page of only the characters not written yet
  const fill = () => { setMode('pages'); setFocus((f) => f + 1) }
  const setStyle = async (print: boolean) => {
    if (!view) return
    const n = picked.size
    await api.setGlyphsPrint(view.meta.id, [...picked], print)
    stopPicking()
    await refresh()
    toast.success(t('hands.styleSet', { n }))
  }
  const deletePicked = async () => {
    if (!view) return
    const ids = [...picked]
    await api.deleteGlyphs(view.meta.id, ids)
    setAskDelete(false)
    stopPicking()
    if (sel && picked.has(sel)) setSel(null)
    await refresh()
    toast.success(t('hands.deleted', { n: ids.length }))
  }

  if (!view) {
    return (
      <>
        <EmptyState art={<NotebookArt />} title={t('hands.empty')} body={t('hands.emptyBody')}
          action={
            <>
              <Button variant="primary" size="lg" onClick={() => void start()}><Plus />{t('hands.start')}</Button>
              <Button size="lg" onClick={async () => { const p = await pickFile('.hand', ['hand']); if (p) await load(await api.importHand(p)) }}><FileInput />{t('hands.importHand')}</Button>
            </>
          } />
        <NameDialog open={naming !== null} onOpenChange={(o) => !o && setNaming(null)} title={t('hands.new')} initial="" onSubmit={create} />
      </>
    )
  }

  const m = view.meta
  const selected = view.glyphs.find((g) => g.id === sel) ?? null
  const flaggedCount = view.glyphs.filter((g) => g.flags.length).length
  const written = view.glyphs.length

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line bg-surface px-3">
        <h1 className="sr-only">{t('hands.title')}</h1>
        <div className="w-60">
          <Select label={t('hands.title')} value={m.id} onValueChange={(id) => { setSel(null); void load(id) }} options={hands.map((h) => ({ value: h.id, label: `${h.name} (${h.count})` }))} />
        </div>
        <Menu>
          <MenuTrigger asChild><Button variant="ghost" size="icon" aria-label={t('common.open')}><ChevronDown /></Button></MenuTrigger>
          <MenuContent>
            <MenuItem onSelect={() => setNaming('new')}><Plus />{t('hands.new')}</MenuItem>
            <MenuItem onSelect={() => setNaming('rename')}><Pencil />{t('common.rename')}</MenuItem>
            <MenuSeparator />
            <MenuItem onSelect={async () => { const p = await pickFile('.hand', ['hand']); if (p) await load(await api.importHand(p)) }}><FileInput />{t('hands.importHand')}</MenuItem>
            <MenuItem onSelect={async () => { const p = await saveAs(`${m.name}.hand`, '.hand', ['hand']); if (p) { await api.exportHand(m.id, p); toast.success(p) } }}><FileOutput />{t('hands.exportHand')}</MenuItem>
            <MenuSeparator />
            <MenuItem danger onSelect={() => setConfirm(true)}><Trash2 />{t('common.delete')}</MenuItem>
          </MenuContent>
        </Menu>
        <div className="flex-1" />
        <Segmented<'pages' | 'letters'> label={t('hands.title')} value={mode} onChange={setMode} options={[
          { value: 'pages', label: <><NotebookPen />{t('hands.guide.title')}</> },
          { value: 'letters', label: <><CaseSensitive />{t('hands.lettersTab')}</> },
        ]} />
      </header>

      {mode === 'pages' ? (
        <div className="min-h-0 flex-1"><Guide hand={view} onChanged={refresh} focus={focus} onOwnPage={() => setPhoto(true)} /></div>
      ) : (
      <div className="flex min-h-0 flex-1">
        <aside className="w-56 shrink-0 overflow-auto border-r border-line bg-surface px-4 py-3 lg:w-64" aria-label={t('hands.coverage')}>
          <h2 className="text-[0.85rem] font-semibold">{t('hands.coverage')}</h2>
          <p className="mb-3 text-[0.8rem] text-muted">{t('hands.letters', { n: written })}</p>
          {view.coverage.map((c) => {
            const have = c.chars.filter((x) => x.n > 0).length
            const missing = c.chars.filter((x) => x.n === 0).map((x) => x.c)
            const name = t(`hands.sheet.${c.group}`)
            return (
              <div key={c.group} className="mb-4">
                <div className="mb-1 flex items-baseline justify-between text-[0.82rem]">
                  <span>{name}</span>
                  <span className="tabular-nums text-muted">{have}/{c.chars.length}</span>
                </div>
                <Progress value={(100 * have) / Math.max(1, c.chars.length)} label={name} />
                {missing.length > 0 && missing.length < c.chars.length && (
                  <p className="mt-1 break-all text-[0.82rem] leading-relaxed text-redpen" aria-label={`${t('hands.missing')}: ${missing.join(' ')}`}>
                    {missing.join(' ')}
                  </p>
                )}
              </div>
            )
          })}
          {view.refill > 0 && written > 0 && (
            <Button size="sm" className="mb-2 w-full" onClick={fill}><NotebookPen />{t('hands.printMissing', { n: view.refill })}</Button>
          )}
          <h2 className="mb-2 mt-5 text-[0.85rem] font-semibold">{t('hands.stats.xh')}</h2>
          <p className="text-muted">{t('hands.stats.mm', { v: m.xh_mm.toFixed(1) })}</p>
          <h2 className="mb-1 mt-4 text-[0.85rem] font-semibold">{t('hands.stats.join')}</h2>
          <Slider label={t('hands.stats.join')} value={Math.round(m.stats.join * 100)} min={0} max={100} step={1} format={(x) => `${x}%`}
            onChange={(x) => { setView({ ...view, meta: { ...m, stats: { ...m.stats, join: x / 100 } } }); void api.setHandStats(m.id, x / 100) }} />
        </aside>

        <section className="flex min-w-0 flex-1 flex-col" aria-label={t('hands.title')}>
          <div className="flex min-h-11 flex-wrap items-center gap-x-3 gap-y-1.5 border-b border-line px-4 py-1.5">
            {picking ? (
              <>
                <span className="text-[0.85rem] font-medium tabular-nums" aria-live="polite">{t('hands.selected', { n: picked.size })}</span>
                <Button size="sm" variant="quiet" onClick={() => pick(shown, true)} disabled={shown.length === 0}>{t('hands.selectShown')}</Button>
                <div className="flex-1" />
                <Button size="sm" variant="quiet" disabled={picked.size === 0} onClick={() => void setStyle(true)}>{t('hands.makePrint')}</Button>
                <Button size="sm" variant="quiet" disabled={picked.size === 0} onClick={() => void setStyle(false)}>{t('hands.makeJoined')}</Button>
                <Button size="sm" variant="danger" disabled={picked.size === 0} onClick={() => setAskDelete(true)}><Trash2 />{t('common.delete')}</Button>
                <Button size="sm" variant="quiet" onClick={stopPicking}><X />{t('common.cancel')}</Button>
              </>
            ) : (
              <>
                <Segmented<Filter> label={t('hands.all')} value={filter} onChange={setFilter} options={[
                  { value: 'all', label: t('hands.all') },
                  { value: 'flagged', label: `${t('hands.flagged')} (${flaggedCount})` },
                  { value: 'off', label: t('common.off') },
                ]} />
                <div className="relative w-32 lg:w-44">
                  <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted" aria-hidden />
                  <Input value={find} onChange={(e) => setFind(e.target.value)} placeholder={t('hands.find')} aria-label={t('hands.find')} className="pl-7" />
                </div>
                <div className="flex-1" />
                <Button size="sm" variant="quiet" onClick={() => setPicking(true)}><ListChecks />{t('hands.select')}</Button>
              </>
            )}
          </div>
          <div ref={grid} className="min-h-0 flex-1 overflow-auto bg-paper-2 px-4 py-3" onKeyDown={(e) => { if (e.key === 'Escape' && picking) stopPicking() }}>
            {groups.length === 0 ? (
              find.trim() ? <p className="py-10 text-center text-muted">{t('hands.noMatch')}</p>
                : <EmptyState title={t('hands.empty')} body={t('hands.emptyBody')} action={<Button variant="primary" onClick={() => setMode('pages')}><NotebookPen />{t('hands.guide.title')}</Button>} />
            ) : (
              <LetterTree hand={m.id} groups={groups} v={v} sel={sel} onSel={setSel} picking={picking} picked={picked} onPick={pick}
                openAll={find.trim() !== '' || filter !== 'all'} onFill={fill} />
            )}
          </div>
        </section>

        <aside className="w-72 shrink-0 border-l border-line bg-surface lg:w-[21rem]" aria-label={t('hands.repair.title')}>
          {selected ? (
            <RepairPanel key={selected.id} hand={m.id} g={selected} onChanged={refresh} onDeleted={() => { setSel(null); void refresh() }} />
          ) : (
            <div className="flex h-full items-center justify-center p-6 text-center text-muted">{t('hands.repair.pick')}</div>
          )}
        </aside>
      </div>
      )}

      <PhotoDialog open={photo} onOpenChange={(o) => { setPhoto(o); if (!o) setGiven(null) }} hand={m.id} onImported={refresh} given={given} givenPrint={givenPrint} />
      <NameDialog open={naming !== null} onOpenChange={(o) => !o && setNaming(null)} title={naming === 'rename' ? t('common.rename') : t('hands.new')} initial={naming === 'rename' ? m.name : ''}
        onSubmit={async (n) => { if (naming === 'rename') { await api.renameHand(m.id, n); setNaming(null); await load(m.id) } else await create(n) }} />
      <Dialog open={askDelete} onOpenChange={setAskDelete} title={t('hands.deleteMany', { n: picked.size })} description={t('hands.deleteManyBody')}
        footer={<><Button variant="quiet" onClick={() => setAskDelete(false)}>{t('common.cancel')}</Button><Button variant="danger" onClick={() => void deletePicked()}><Trash2 />{t('common.delete')}</Button></>}>
        <span />
      </Dialog>
      <Dialog open={confirm} onOpenChange={setConfirm} title={t('common.delete')} description={t('common.confirmDelete', { name: m.name })}
        footer={<><Button variant="quiet" onClick={() => setConfirm(false)}>{t('common.cancel')}</Button><Button variant="danger" onClick={async () => { await api.deleteHand(m.id); setConfirm(false); setSel(null); await load(null) }}>{t('common.delete')}</Button></>}>
        <span />
      </Dialog>
    </div>
  )
}
