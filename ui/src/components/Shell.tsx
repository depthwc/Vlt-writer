import { Command } from 'cmdk'
import { FilePlus2, FileUp, Images, Languages, Moon, NotebookPen, ScanLine, Settings, Signature, Sparkles, StickyNote } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Kbd, Progress } from '@/components/ui/controls'
import { Dialog, Tip } from '@/components/ui/overlay'
import { LANGS } from '@/i18n'
import { api } from '@/lib/api'
import { cn } from '@/lib/utils'
import { useApp, type Workspace } from '@/state/app'
import { useJobs } from '@/state/jobs'

const NAV: { ws: Workspace; icon: React.ElementType; key: string; kbd: string }[] = [
  { ws: 'write', icon: NotebookPen, key: 'nav.write', kbd: 'Ctrl+1' },
  { ws: 'hands', icon: Signature, key: 'nav.hands', kbd: 'Ctrl+2' },
  { ws: 'papers', icon: StickyNote, key: 'nav.papers', kbd: 'Ctrl+3' },
]

function JobIndicator() {
  const { t } = useTranslation()
  const jobs = Object.values(useJobs((s) => s.jobs))
  if (!jobs.length) return null
  const j = jobs[0]
  const pct = j.total ? (100 * j.done) / j.total : 0
  return (
    <div className="w-full px-2" role="status" aria-live="polite">
      <div className="mb-1 truncate text-center text-[0.65rem] text-muted" title={j.message}>{t(`jobs.${j.label}`)}</div>
      <Progress value={pct} label={t(`jobs.${j.label}`)} />
      <button className="mt-1 w-full text-[0.65rem] text-muted hover:text-redpen" onClick={() => api.cancelJob(j.id)}>{t('jobs.cancel')}</button>
    </div>
  )
}

export function Rail() {
  const { t } = useTranslation()
  const ws = useApp((s) => s.ws)
  const setWs = useApp((s) => s.setWs)
  const item = (w: Workspace, Icon: React.ElementType, label: string, kbd: string) => (
    <Tip label={label} shortcut={kbd} side="right" key={w}>
      <button
        onClick={() => setWs(w)}
        aria-current={ws === w ? 'page' : undefined}
        className={cn(
          'relative flex w-full flex-col items-center gap-1 rounded-lg py-2 text-[0.68rem] text-muted transition-colors hover:text-graphite',
          ws === w && 'bg-ink-soft text-ink before:absolute before:-left-2 before:top-2 before:bottom-2 before:w-[3px] before:rounded-r before:bg-ink',
        )}
      >
        <Icon className="size-5" strokeWidth={1.75} />
        <span className="leading-tight">{label}</span>
      </button>
    </Tip>
  )
  return (
    <nav aria-label="Main" className="flex w-[4.75rem] shrink-0 flex-col items-center gap-1 border-r border-line bg-surface px-2 py-3">
      {NAV.map((n) => item(n.ws, n.icon, t(n.key), n.kbd))}
      <div className="flex-1" />
      <JobIndicator />
      {item('settings', Settings, t('nav.settings'), 'Ctrl+,')}
    </nav>
  )
}

export function CommandPalette({ actions }: { actions: { id: string; label: string; icon: React.ElementType; run: () => void; kbd?: string }[] }) {
  const { t } = useTranslation()
  const open = useApp((s) => s.paletteOpen)
  const setOpen = useApp((s) => s.setPaletteOpen)
  return (
    <Command.Dialog
      open={open}
      onOpenChange={setOpen}
      label={t('nav.palette')}
      overlayClassName="fixed inset-0 z-40 bg-graphite/25"
      contentClassName="fixed left-1/2 top-[18vh] z-50 w-[min(560px,92vw)] -translate-x-1/2 overflow-hidden rounded-xl border border-line bg-surface shadow-pop"
    >
      <Command.Input autoFocus placeholder={t('nav.palette')} className="h-12 w-full border-b border-line bg-transparent px-4 text-[0.95rem] outline-none placeholder:text-muted" />
      <Command.List className="max-h-80 overflow-auto p-1.5">
        <Command.Empty className="px-3 py-6 text-center text-muted">—</Command.Empty>
        {actions.map((a) => (
          <Command.Item
            key={a.id}
            value={a.label}
            onSelect={() => {
              setOpen(false)
              a.run()
            }}
            className="flex h-9 cursor-default items-center gap-2.5 rounded-md px-2.5 text-[0.88rem] data-[selected=true]:bg-ink-soft [&_svg]:size-4 [&_svg]:text-muted"
          >
            <a.icon />
            <span className="flex-1">{a.label}</span>
            {a.kbd && <Kbd>{a.kbd}</Kbd>}
          </Command.Item>
        ))}
      </Command.List>
    </Command.Dialog>
  )
}

export function usePaletteActions(handlers: { newDoc: () => void; importDoc: () => void; exportDoc: () => void; printSheet: () => void; addPaper: () => void }) {
  const { t } = useTranslation()
  const s = useApp()
  return [
    { id: 'new', label: t('write.newPage'), icon: FilePlus2, run: () => { s.setWs('write'); handlers.newDoc() }, kbd: 'Ctrl+N' },
    { id: 'import', label: t('write.import'), icon: FileUp, run: () => { s.setWs('write'); handlers.importDoc() }, kbd: 'Ctrl+O' },
    { id: 'export', label: t('write.exportBtn'), icon: Images, run: handlers.exportDoc, kbd: 'Ctrl+E' },
    { id: 'samples', label: t('hands.guide.title'), icon: NotebookPen, run: () => { s.setWs('hands'); handlers.printSheet() } },
    { id: 'paper', label: t('papers.add'), icon: ScanLine, run: () => { s.setWs('papers'); handlers.addPaper() } },
    { id: 'go-write', label: t('nav.write'), icon: NotebookPen, run: () => s.setWs('write'), kbd: 'Ctrl+1' },
    { id: 'go-hands', label: t('nav.hands'), icon: Signature, run: () => s.setWs('hands'), kbd: 'Ctrl+2' },
    { id: 'go-papers', label: t('nav.papers'), icon: StickyNote, run: () => s.setWs('papers'), kbd: 'Ctrl+3' },
    { id: 'go-settings', label: t('nav.settings'), icon: Settings, run: () => s.setWs('settings'), kbd: 'Ctrl+,' },
    { id: 'theme', label: `${t('settings.theme')}: ${t(`settings.themes.${s.theme === 'dark' ? 'light' : 'dark'}`)}`, icon: Moon, run: () => s.setTheme(s.theme === 'dark' ? 'light' : 'dark') },
    ...LANGS.filter((l) => l.code !== s.lang).map((l) => ({ id: `lang-${l.code}`, label: `${t('settings.language')}: ${l.label}`, icon: Languages, run: () => s.setLang(l.code) })),
    { id: 'tour', label: t('settings.tour'), icon: Sparkles, run: () => s.setTourOpen(true) },
  ]
}

export function EmptyState({ title, body, action, art }: { title: string; body: string; action?: React.ReactNode; art?: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      {art}
      <h2 className="max-w-md text-[1.15rem] font-semibold">{title}</h2>
      <p className="max-w-md text-muted">{body}</p>
      {action && <div className="mt-2 flex gap-2">{action}</div>}
    </div>
  )
}

/** A strip of squared paper with a written stroke: the art for empty states. */
export function NotebookArt() {
  return (
    <svg viewBox="0 0 220 120" className="mb-2 w-56" aria-hidden>
      <rect x="1" y="1" width="218" height="118" rx="10" fill="var(--surface)" stroke="var(--line)" />
      {Array.from({ length: 10 }, (_, i) => <path key={`v${i}`} d={`M${22 * i + 11} 1v118`} stroke="var(--grid)" />)}
      {Array.from({ length: 5 }, (_, i) => <path key={`h${i}`} d={`M1 ${22 * i + 16}h218`} stroke="var(--grid)" />)}
      <path d="M33 1v118" stroke="var(--line)" />
      <path d="M48 74c7 0 10-10 13-18s9-17 14-16c5 2-5 23-10 34 4-7 10-10 13-9 5 2 1 9 4 9s7-10 11-10c4 0 2 8 5 9 3 1 8-7 11-7 4 0 1 9 5 9 5 0 8-6 12-6 5 0 3 7 7 7 3 0 6-3 9-5" fill="none" stroke="var(--ink)" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function Tour() {
  const { t } = useTranslation()
  const open = useApp((s) => s.tourOpen)
  const setOpen = useApp((s) => s.setTourOpen)
  const [i, setI] = React.useState(0)
  const steps = [1, 2, 3, 4]
  const close = () => {
    setOpen(false)
    setI(0)
    void api.setSetting('tourDone', '1').catch(() => {})
  }
  return (
    <Dialog
      open={open}
      onOpenChange={(v) => (v ? setOpen(true) : close())}
      title={t(`tour.s${steps[i]}t`)}
      width="max-w-md"
      footer={
        <>
          <div className="mr-auto flex items-center gap-1.5" aria-hidden>
            {steps.map((s, k) => <span key={s} className={cn('size-1.5 rounded-full bg-line', k === i && 'bg-ink')} />)}
          </div>
          <Button variant="quiet" onClick={close}>{t('tour.skip')}</Button>
          {i < steps.length - 1 ? <Button variant="primary" onClick={() => setI(i + 1)}>{t('common.next')}</Button> : <Button variant="primary" onClick={close}>{t('tour.start')}</Button>}
        </>
      }
    >
      <div className="flex flex-col items-center gap-3 pb-2">
        <NotebookArt />
        <p className="text-center text-[0.95rem] leading-relaxed">{t(`tour.s${steps[i]}`)}</p>
      </div>
    </Dialog>
  )
}
