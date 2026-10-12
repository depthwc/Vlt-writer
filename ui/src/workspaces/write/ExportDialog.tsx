import { pictureDir, join } from '@tauri-apps/api/path'
import { FolderOpen } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import { Checkbox, Input, Progress, Row, Select, Slider, Switch } from '@/components/ui/controls'
import { Dialog } from '@/components/ui/overlay'
import { api, inTauri, type Project } from '@/lib/api'
import { pickFolder } from '@/lib/files'
import { runJob } from '@/state/jobs'
import { usePref } from '@/state/prefs'

type Fmt = 'jpg' | 'png' | 'pdf' | 'svg' | 'gcode'

function parsePages(s: string, total: number): number[] | null {
  if (!s.trim()) return null
  const out = new Set<number>()
  for (const part of s.split(',')) {
    const [a, b] = part.split('-').map((x) => parseInt(x.trim(), 10))
    if (Number.isNaN(a)) continue
    for (let i = a; i <= (Number.isNaN(b) ? a : b); i++) if (i >= 1 && i <= total) out.add(i - 1)
  }
  return [...out].sort((x, y) => x - y)
}

export function ExportDialog({ open, onOpenChange, project, pages }: { open: boolean; onOpenChange: (v: boolean) => void; project: Project; pages: number }) {
  const { t } = useTranslation()
  const [fmts, setFmts] = usePref<Fmt[]>('export.formats', ['jpg'])
  const [range, setRange] = React.useState('')
  const [shots, setShots] = usePref('export.shots', 1)
  const [random, setRandom] = usePref('export.random', true)
  const [quality, setQuality] = usePref('export.quality', 90)
  const [width, setWidth] = usePref('export.width', '0')
  const [folder, setFolder] = usePref('export.folder', '')
  const [template, setTemplate] = usePref('export.template', '{name}_p{page}_{n}')
  const [penUp, setPenUp] = usePref('export.penUp', 'M3 S0')
  const [penDown, setPenDown] = usePref('export.penDown', 'M3 S90')
  const [feed, setFeed] = usePref('export.feed', 2000)
  const [busy, setBusy] = React.useState<{ done: number; total: number } | null>(null)
  const [done, setDone] = React.useState<{ files: string[]; folder: string } | null>(null)

  React.useEffect(() => {
    if (!open) return
    setDone(null)
    if (!folder && inTauri()) void pictureDir().then((d) => join(d, 'Vlt-writer')).then(setFolder)
  }, [open, folder])

  const photos = fmts.some((f) => f === 'jpg' || f === 'png' || f === 'pdf')
  const toggle = (f: Fmt, on: boolean) => setFmts(on ? [...fmts, f] : fmts.filter((x) => x !== f))

  const start = async () => {
    setBusy({ done: 0, total: 1 })
    try {
      const r = await runJob<{ files: string[]; folder: string }>(
        () => api.exportProject({
          project: project.id,
          paper: project.paper,
          formats: fmts,
          pages: parsePages(range, pages),
          shots_per_page: shots,
          random,
          quality,
          width: Number(width),
          folder,
          template,
          style: project.shot,
          gcode: { pen_up: penUp, pen_down: penDown, feed, travel: 4000, dwell: 0.15, flip_y: true, page_h: 297, offset: [0, 0] },
        }),
        (e) => setBusy({ done: e.done, total: Math.max(1, e.total) }),
      )
      setDone(r)
      toast.success(t('exportDlg.done', { n: r.files.length }), { action: { label: t('exportDlg.openFolder'), onClick: () => void api.openFolder(r.folder) } })
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    } finally {
      setBusy(null)
    }
  }

  const fmt = (f: Fmt, label: string, hint?: string) => (
    <label className="flex items-start gap-2.5 py-1">
      <Checkbox checked={fmts.includes(f)} onCheckedChange={(v) => toggle(f, v)} label={label} />
      <span>
        <span className="block leading-tight">{label}</span>
        {hint && <span className="block text-[0.78rem] text-muted">{hint}</span>}
      </span>
    </label>
  )

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title={t('exportDlg.title')} description={project.name} width="max-w-2xl"
      footer={
        <>
          {busy && <div className="mr-auto w-48" role="status"><Progress value={(100 * busy.done) / busy.total} label={t('jobs.export')} /></div>}
          {done && <Button variant="quiet" className="mr-auto" onClick={() => void api.openFolder(done.folder)}><FolderOpen />{t('exportDlg.openFolder')}</Button>}
          <Button variant="quiet" onClick={() => onOpenChange(false)}>{t('common.close')}</Button>
          <Button variant="primary" onClick={start} disabled={!fmts.length || !folder || !!busy || (photos && !project.paper)}>{busy ? t('common.working') : t('exportDlg.start')}</Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-x-8 pb-3">
        <div>
          <h3 className="mb-1 font-semibold">{t('exportDlg.formats')}</h3>
          {fmt('jpg', 'JPG', t('exportDlg.photoHint'))}
          {fmt('png', 'PNG')}
          {fmt('pdf', t('exportDlg.pdf'))}
          {fmt('svg', t('exportDlg.svg'))}
          {fmt('gcode', t('exportDlg.gcode'))}
          {photos && !project.paper && <p className="pt-1 text-[0.8rem] text-redpen">{t('exportDlg.needPaper')}</p>}
        </div>
        <div>
          <Row label={t('exportDlg.pages')}><Input value={range} onChange={(e) => setRange(e.target.value)} placeholder={`${t('exportDlg.allPages')} (1-${pages})`} aria-label={t('exportDlg.pages')} /></Row>
          {photos && (
            <>
              <Row label={t('exportDlg.shots')}><Slider label={t('exportDlg.shots')} value={shots} min={1} max={10} step={1} onChange={setShots} /></Row>
              <label className="flex items-center gap-2 py-1.5"><Switch checked={random} onCheckedChange={setRandom} label={t('exportDlg.random')} /><span className="text-[0.82rem]">{t('exportDlg.random')}</span></label>
              <Row label={t('exportDlg.quality')}><Slider label={t('exportDlg.quality')} value={quality} min={60} max={100} step={1} onChange={setQuality} /></Row>
              <Row label={t('exportDlg.width')}>
                <Select label={t('exportDlg.width')} value={width} onValueChange={setWidth} options={[{ value: '0', label: t('exportDlg.original') }, ...['3000', '2400', '1600', '1200'].map((w) => ({ value: w, label: `${w} px` }))]} />
              </Row>
            </>
          )}
        </div>
      </div>
      <div className="space-y-1 border-t border-line pt-3">
        <Row label={t('exportDlg.folder')}>
          <div className="flex gap-1.5">
            <Input value={folder} onChange={(e) => setFolder(e.target.value)} aria-label={t('exportDlg.folder')} />
            <Button onClick={async () => { const f = await pickFolder(); if (f) setFolder(f) }}>{t('exportDlg.choose')}</Button>
          </div>
        </Row>
        <Row label={t('exportDlg.names')}><Input value={template} onChange={(e) => setTemplate(e.target.value)} aria-label={t('exportDlg.names')} /></Row>
        {fmts.includes('gcode') && (
          <div className="grid grid-cols-3 gap-2 pt-2">
            <label className="text-[0.8rem] text-muted">{t('exportDlg.penUp')}<Input value={penUp} onChange={(e) => setPenUp(e.target.value)} className="mt-1" /></label>
            <label className="text-[0.8rem] text-muted">{t('exportDlg.penDown')}<Input value={penDown} onChange={(e) => setPenDown(e.target.value)} className="mt-1" /></label>
            <label className="text-[0.8rem] text-muted">{t('exportDlg.feed')}<Input type="number" value={feed} onChange={(e) => setFeed(Number(e.target.value))} className="mt-1" /></label>
          </div>
        )}
      </div>
    </Dialog>
  )
}
