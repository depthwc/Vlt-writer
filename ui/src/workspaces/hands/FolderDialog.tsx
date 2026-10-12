import { Check, FolderOpen, TriangleAlert } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/controls'
import { Dialog } from '@/components/ui/overlay'
import { api, SHARP_PX_MM, type FolderScan, type Report } from '@/lib/api'
import { runJob } from '@/state/jobs'

type Done = { file: string; report: Report }[]

/** A folder of pages written before: each photo goes with the .txt of the same name. */
export function FolderDialog({ hand, dir, onClose, onImported }: { hand: string; dir: string | null; onClose: () => void; onImported: () => void }) {
  const { t } = useTranslation()
  const [scan, setScan] = React.useState<FolderScan | null>(null)
  const [busy, setBusy] = React.useState<{ done: number; total: number } | null>(null)
  const [done, setDone] = React.useState<Done | null>(null)

  React.useEffect(() => {
    setScan(null)
    setDone(null)
    if (dir) api.scanFolder(hand, dir).then(setScan, (e) => toast.error(t('errors.generic', { msg: String(e) })))
  }, [hand, dir, t])

  const run = async () => {
    if (!scan) return
    setBusy({ done: 0, total: scan.pages.length })
    try {
      const r = await runJob<Done>(() => api.importOwnPages(hand, scan.pages.map((p) => ({ images: p.images, text: p.text }))), (e) => setBusy({ done: e.done, total: e.total }))
      setDone(r)
      onImported()
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    } finally {
      setBusy(null)
    }
  }
  const letters = done?.reduce((n, d) => n + (d.report.ok ? d.report.glyphs : 0), 0) ?? 0
  const n = scan?.pages.length ?? 0

  return (
    <Dialog open={!!dir} onOpenChange={(o) => !o && !busy && onClose()} title={t('hands.folder.title')} description={t('hands.folder.body')} width="max-w-2xl"
      footer={
        <>
          <Button variant="quiet" onClick={onClose} disabled={!!busy}>{t('common.close')}</Button>
          {!done && <Button variant="primary" disabled={!n || !!busy} onClick={() => void run()}>{busy ? t('common.working') : t('hands.folder.import', { n })}</Button>}
        </>
      }>
      <div className="space-y-3 pb-1">
        <p className="flex items-center gap-2 truncate text-[0.82rem] text-muted"><FolderOpen className="size-4 shrink-0" /><span className="truncate">{dir}</span></p>
        {busy && <Progress value={busy.total ? (100 * busy.done) / busy.total : 0} label={t('common.working')} />}
        {!scan ? (
          <p className="text-muted">{t('common.loading')}</p>
        ) : done ? (
          <>
            <p className="flex items-center gap-1.5 font-medium text-ok" role="status"><Check className="size-4" />{t('hands.folder.done', { n: letters, pages: done.filter((d) => d.report.ok).length })}</p>
            {done.some((d) => d.report.ok && d.report.px_mm > 0 && d.report.px_mm < SHARP_PX_MM) && <p className="text-[0.8rem] text-redpen">{t('hands.photo.small')}</p>}
            <ul className="max-h-80 space-y-0.5 overflow-auto text-[0.84rem]">
              {done.map((d, i) => (
                <li key={i} className="flex items-center gap-2">
                  {d.report.ok ? <Check className="size-4 shrink-0 text-ok" /> : <TriangleAlert className="size-4 shrink-0 text-redpen" />}
                  <span className="min-w-0 flex-1 truncate">{scan?.pages[i]?.name ?? d.file}</span>
                  {d.report.ok && d.report.px_mm > 0 && d.report.px_mm < SHARP_PX_MM && <TriangleAlert className="size-3.5 shrink-0 text-redpen" aria-label={t('hands.photo.small')} />}
                  <span className="text-[0.78rem] tabular-nums text-muted">{d.report.ok ? d.report.glyphs : d.report.error}</span>
                </li>
              ))}
            </ul>
          </>
        ) : n === 0 ? (
          <p className="text-muted">{t('hands.folder.none')}</p>
        ) : (
          <ul className="max-h-80 space-y-0.5 overflow-auto text-[0.84rem]">
            {scan.pages.map((p) => (
              <li key={p.images[0]} className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate">{p.name}</span>
                {p.again && <span className="text-[0.75rem] text-muted">{t('hands.folder.again')}</span>}
                <span className="text-[0.78rem] tabular-nums text-muted">{t('hands.photo.lines', { n: p.lines })}</span>
              </li>
            ))}
          </ul>
        )}
        {scan && !done && scan.no_text.length > 0 && <p className="text-[0.8rem] text-muted">{t('hands.folder.noText', { names: scan.no_text.slice(0, 8).join(', ') + (scan.no_text.length > 8 ? '…' : '') })}</p>}
        {scan && !done && scan.no_photo.length > 0 && <p className="text-[0.8rem] text-muted">{t('hands.folder.noPhoto', { names: scan.no_photo.slice(0, 8).join(', ') + (scan.no_photo.length > 8 ? '…' : '') })}</p>}
      </div>
    </Dialog>
  )
}
