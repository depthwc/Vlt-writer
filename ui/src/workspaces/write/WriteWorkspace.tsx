import { ChevronDown, ChevronLeft, ChevronRight, FilePlus2, FileUp, Images, Signature, StickyNote, Trash2 } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { EmptyState, NotebookArt } from '@/components/Shell'
import { Button } from '@/components/ui/button'
import { Input, Segmented } from '@/components/ui/controls'
import { Dialog, Menu, MenuContent, MenuItem, MenuSeparator, MenuTrigger, Tip } from '@/components/ui/overlay'
import { api, NOTEBOOK, RANDOM, type LayoutInfo, type PaperMeta, type Project, type Summary } from '@/lib/api'
import { blocksToDoc } from '@/lib/blocks'
import { DOC_EXT, pickFile, useFileDrop } from '@/lib/files'
import { debounce } from '@/lib/utils'
import { useApp } from '@/state/app'
import { useRequest } from '@/state/bus'

import { DocEditor, type DocEditorHandle } from './DocEditor'
import { ExportDialog } from './ExportDialog'
import { Inspector } from './Inspector'
import { onBeforeClose } from '@/state/closing'
import { usePref } from '@/state/prefs'

import { CleanGallery, PhotoGallery, type GalleryHandle } from './PageView'

type Mode = 'photo' | 'svg'

export default function WriteWorkspace() {
  const { t } = useTranslation()
  const app = useApp()
  const [projects, setProjects] = React.useState<Summary[]>([])
  const [hands, setHands] = React.useState<Summary[]>([])
  const [papers, setPapers] = React.useState<PaperMeta[]>([])
  const [project, setProject] = React.useState<Project | null>(null)
  const [info, setInfo] = React.useState<LayoutInfo | null>(null)
  const [version, setVersion] = React.useState(0)
  const [page, setPage] = React.useState(0)
  const [mode, setMode] = usePref<Mode>('write.view', 'photo')
  const [preview, setPreview] = React.useState({ random: false, seed: 1 })
  const [presets, setPresets] = React.useState<[string, [number, number, number]][]>([])
  const [handInk, setHandInk] = React.useState<[number, number, number] | null>(null)
  const [confirm, setConfirm] = React.useState(false)
  const editor = React.useRef<DocEditorHandle>(null)
  const gallery = React.useRef<GalleryHandle>(null)
  const latest = React.useRef<Project | null>(null)

  // pages and the paper of each (a notebook's or random papers differ from page to page)
  const refreshInfo = React.useCallback(async (p: Project) => {
    try {
      const i = await api.layoutInfo(p.id, p.paper, 0)
      setInfo(i)
      setPage((pg) => Math.min(pg, Math.max(0, i.pages - 1)))
    } catch {
      setInfo(null)
    }
  }, [])

  const save = React.useMemo(() => debounce(async (p: Project) => {
    try {
      const saved = await api.saveProject(p)
      // keep newer local edits; take only what the backend derives
      if (latest.current?.id === saved.id) latest.current = { ...latest.current, blocks: saved.blocks }
      await refreshInfo(saved)
      setVersion((v) => v + 1)
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    }
  }, 450), [refreshInfo, t])

  // closing the window: the last edits are saved first
  React.useEffect(() => onBeforeClose(() => (latest.current ? save.flush(latest.current) : undefined)), [save])
  // the document's name in the title bar
  const setDocName = useApp((s) => s.setDocName)
  React.useEffect(() => {
    setDocName(project?.name ?? null)
  }, [project?.name, setDocName])
  React.useEffect(() => () => setDocName(null), [setDocName])

  const update = React.useCallback((patch: Partial<Project>) => {
    // from the latest document, once: saving inside a state update could run twice
    const cur = latest.current
    if (!cur) return
    const next = { ...cur, ...patch }
    latest.current = next
    setProject(next)
    save(next)
  }, [save])

  const open = React.useCallback(async (id: string) => {
    if (latest.current) save.flush(latest.current)
    let p = await api.getProject(id)
    // a document made before any handwriting/paper existed: use the first available
    if (!p.hand || !p.paper) {
      const [h, pa] = await Promise.all([api.listHands(), api.listPapers()])
      const hand = p.hand ?? h[0]?.id ?? null
      // a notebook when its pages are there
      const paper = p.paper ?? (pa.some((x) => x.side !== 'any') ? NOTEBOOK : pa[0]?.id ?? null)
      if (hand !== p.hand || paper !== p.paper) p = await api.saveProject({ ...p, hand, paper })
    }
    latest.current = p
    setProject(p)
    app.setProjectId(id)
    setPage(0)
    await refreshInfo(p)
    setVersion((v) => v + 1)
  }, [app, refreshInfo, save])

  const loadLists = React.useCallback(async () => {
    const [pr, h, pa] = await Promise.all([api.listProjects(), api.listHands(), api.listPapers()])
    setProjects(pr)
    setHands(h)
    setPapers(pa)
    return { pr, h, pa }
  }, [])

  const newDoc = React.useCallback(async () => {
    const p = await api.createProject(t('write.newDocName'), app.handId)
    await loadLists()
    await open(p.id)
  }, [app.handId, loadLists, open, t])

  React.useEffect(() => {
    void (async () => {
      const { pr } = await loadLists()
      const id = pr.find((p) => p.id === app.projectId)?.id ?? pr[0]?.id
      if (id) await open(id)
      else await newDoc()
      setPresets(await api.penPresets())
    })()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  React.useEffect(() => {
    if (!project?.hand) return
    void api.getHand(project.hand).then((h) => setHandInk(h.meta.ink)).catch(() => setHandInk(null))
  }, [project?.hand])

  const importDoc = React.useCallback(async (path?: string) => {
    const f = path ?? (await pickFile(t('write.importHint'), DOC_EXT))
    if (!f || !project) return
    try {
      const blocks = await api.importDocument(f)
      editor.current?.setDoc(blocksToDoc(blocks))
      const name = f.split(/[\\/]/).pop()!.replace(/\.\w+$/, '')
      if (project.name === t('write.newDocName')) update({ name })
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    }
  }, [project, t, update])

  useRequest('newDoc', () => void newDoc())
  useRequest('importDoc', () => void importDoc())

  const over = useFileDrop(true, DOC_EXT, (p) => void importDoc(p[0]))

  if (!project) return <div className="p-8 text-muted">{t('common.loading')}</div>

  // clean pages are A4, the paper's are its own size: their counts differ
  const pages = (mode === 'svg' ? info?.clean_pages : info?.pages) ?? 1
  const hasHand = hands.length > 0 && !!project.hand
  const paper = project.paper === NOTEBOOK ? (papers.some((p) => p.side !== 'any') ? NOTEBOOK : undefined)
    : project.paper === RANDOM ? (papers.length ? RANDOM : undefined)
    : papers.find((p) => p.id === project.paper)?.id

  const center = () => {
    if (!hasHand) {
      return <EmptyState art={<NotebookArt />} title={t('write.noHand')} body={t('write.noHandBody')} action={<Button variant="primary" size="lg" onClick={() => app.setWs('hands')}><Signature />{t('write.goHands')}</Button>} />
    }
    if (mode === 'svg') {
      return <CleanGallery ref={gallery} project={project.id} pages={pages} version={version} onPage={setPage} onSwap={async (w, c) => {
        const gid = await api.cycleGlyph(project.id, w, c)
        if (gid) {
          const fresh = await api.getProject(project.id)
          update({ params: { ...(latest.current ?? project).params, overrides: fresh.params.overrides } })
        }
      }} />
    }
    if (!paper || !info || !info.sheets.length) {
      return <EmptyState title={t('write.noPaper')} body={t('papers.emptyBody')} action={<Button variant="primary" size="lg" onClick={() => app.setWs('papers')}><StickyNote />{t('write.goPapers')}</Button>} />
    }
    return <PhotoGallery ref={gallery} project={project.id} paper={paper} sheets={info.sheets} pages={pages} version={version} preview={preview} onPage={setPage} />
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line bg-surface px-3">
        <h1 className="sr-only">{t('nav.write')}</h1>
        <Menu>
          <MenuTrigger asChild>
            <Button variant="ghost" size="sm" aria-label={t('write.projects')}>{t('write.projects')}<ChevronDown /></Button>
          </MenuTrigger>
          <MenuContent>
            <MenuItem onSelect={newDoc} shortcut="Ctrl+N"><FilePlus2 />{t('write.newPage')}</MenuItem>
            <MenuItem onSelect={() => void importDoc()} shortcut="Ctrl+O"><FileUp />{t('write.import')}</MenuItem>
            <MenuSeparator />
            {projects.slice(0, 12).map((p) => (
              <MenuItem key={p.id} onSelect={() => void open(p.id)}><span className={p.id === project.id ? 'font-semibold' : ''}>{p.name}</span></MenuItem>
            ))}
            <MenuSeparator />
            <MenuItem danger onSelect={() => setConfirm(true)}><Trash2 />{t('common.delete')}</MenuItem>
          </MenuContent>
        </Menu>
        <Input value={project.name} onChange={(e) => update({ name: e.target.value })} onBlur={() => void loadLists()} aria-label={t('write.name')} className="h-8 w-64 border-transparent bg-transparent text-[0.95rem] font-semibold hover:border-line" />
        <div className="flex-1" />
        <Segmented<Mode> label={t('write.view.photo')} value={mode} onChange={setMode} options={[{ value: 'photo', label: t('write.view.photo') }, { value: 'svg', label: t('write.view.svg') }]} />
        <div className="flex items-center gap-0.5" aria-label={t('common.page', { n: page + 1, total: pages })}>
          <Button variant="ghost" size="icon-sm" disabled={page === 0} onClick={() => gallery.current?.show(page - 1)} aria-label={t('common.back')}><ChevronLeft /></Button>
          <span className="w-14 text-center text-[0.8rem] tabular-nums text-muted">{page + 1} / {pages}</span>
          <Button variant="ghost" size="icon-sm" disabled={page >= pages - 1} onClick={() => gallery.current?.show(page + 1)} aria-label={t('common.next')}><ChevronRight /></Button>
        </div>
        <Tip label={t('write.exportBtn')} shortcut="Ctrl+E">
          <Button variant="primary" onClick={() => app.setExportOpen(true)} disabled={!hasHand}><Images />{t('write.exportBtn')}</Button>
        </Tip>
      </header>
      <div className="flex min-h-0 flex-1">
        <section className={`w-[32%] min-w-[15rem] max-w-[40rem] border-r border-line lg:w-[38%] lg:min-w-[20rem] ${over ? 'ring-4 ring-inset ring-ink/40' : ''}`} aria-label={t('write.name')}>
          <DocEditor key={project.id} ref={editor} doc={project.doc} onChange={(doc, source) => update({ doc, source })} />
        </section>
        <section className="relative min-w-0 flex-1" aria-label={t('write.pageLabel')}>
          {center()}
          {info && hasHand && (info.missing.length > 0 || info.missing_print.length > 0) && (
            <div className="absolute inset-x-0 bottom-0 space-y-1.5 border-t border-redpen/30 bg-redpen-soft px-4 py-2 text-[0.82rem] text-redpen" role="status">
              {([[info.missing, false], [info.missing_print, true]] as const).filter(([list]) => list.length > 0).map(([list, print]) => (
                <div key={String(print)} className="flex items-center gap-3">
                  <span className="min-w-0 flex-1">
                    {t(print ? 'write.missingPrint' : 'write.missing')}{' '}
                    {list.map(([c, n]) => (
                      <span key={c} className="mr-2 inline-flex items-baseline gap-0.5 whitespace-nowrap">
                        <span className="rounded border border-redpen/30 bg-surface px-1 font-semibold text-graphite">{c}</span>
                        {n > 1 && <span className="text-[0.75rem]">×{n}</span>}
                      </span>
                    ))}
                  </span>
                  <Button size="sm" variant="danger" onClick={() => { app.setToWrite({ chars: list.map(([c]) => c).join(''), print }); app.setWs('hands') }}>{t('write.writeThese')}</Button>
                </div>
              ))}
            </div>
          )}
        </section>
        <aside className="w-72 shrink-0 overflow-auto border-l border-line bg-surface lg:w-[19.5rem]" aria-label={t('inspector.style')}>
          <Inspector project={project} update={update} hands={hands} papers={papers} info={info} mode={mode}
            preview={preview} setPreview={setPreview} presets={presets} handInk={handInk} />
        </aside>
      </div>
      <ExportDialog open={app.exportOpen} onOpenChange={app.setExportOpen} project={project} pages={pages} />
      <Dialog open={confirm} onOpenChange={setConfirm} title={t('common.delete')} description={t('common.confirmDelete', { name: project.name })}
        footer={<><Button variant="quiet" onClick={() => setConfirm(false)}>{t('common.cancel')}</Button><Button variant="danger" onClick={async () => {
          await api.deleteProject(project.id)
          setConfirm(false)
          const { pr } = await loadLists()
          if (pr[0]) await open(pr[0].id)
          else await newDoc()
        }}>{t('common.delete')}</Button></>}>
        <span />
      </Dialog>
    </div>
  )
}
