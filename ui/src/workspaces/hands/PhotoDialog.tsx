import { Check, FileText, ImagePlus, TriangleAlert } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { ZoomPan } from '@/components/ZoomPan'
import { Button } from '@/components/ui/button'
import { Label, Segmented, Textarea } from '@/components/ui/controls'
import { Dialog } from '@/components/ui/overlay'
import { api, SHARP_PX_MM, type Report } from '@/lib/api'
import { fileName, IMAGE_EXT, pickFile, pickImages, useFileDrop } from '@/lib/files'
import { cn } from '@/lib/utils'
import { runJob } from '@/state/jobs'

/** A page of the person's own writing: its photo (or photos, when it did not fit on one sheet)
 *  plus its text, typed or from a .txt (one with the photo's name is read by itself). */
export function PhotoDialog({ open, onOpenChange, hand, onImported, given, givenPrint }: { open: boolean; onOpenChange: (v: boolean) => void; hand: string; onImported: () => void; given?: string | null; givenPrint?: boolean }) {
  const { t } = useTranslation()
  const [paths, setPaths] = React.useState<string[]>([])
  const [shown, setShown] = React.useState(0)
  const [preview, setPreview] = React.useState<{ url: string; w: number; h: number } | null>(null)
  const [text, setText] = React.useState('')
  const [busy, setBusy] = React.useState(false)
  const [result, setResult] = React.useState<Report | null>(null)
  const [pages, setPages] = React.useState(0)
  const [from, setFrom] = React.useState<string | null>(null)
  // the letters on the page: the person's joined writing, or print letters
  const [print, setPrint] = React.useState(false)
  // characters a document needs: what to write is given
  React.useEffect(() => {
    if (open && given) {
      setText(given)
      setPrint(!!givenPrint)
    }
  }, [open, given, givenPrint])

  const show = React.useCallback(async (p: string) => {
    setPreview(null)
    try {
      const url = await api.photoPreview(p)
      const img = new Image()
      img.onload = () => setPreview({ url, w: img.naturalWidth, h: img.naturalHeight })
      img.src = url
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    }
  }, [t])
  const choose = React.useCallback(async (ps: string[]) => {
    if (!ps.length) return
    const sorted = [...ps].sort((a, b) => a.localeCompare(b, undefined, { numeric: true }))
    const p = sorted[0]
    setPaths(sorted)
    setShown(0)
    setResult(null)
    void show(p)
    try {
      // the .txt of the same name, if the text is not typed yet
      const own = await api.photoText(p)
      if (own !== null) {
        setText((cur) => (cur.trim() ? cur : own))
        setFrom((cur) => cur ?? fileName(p).replace(/\.[^.]+$/, '.txt'))
      }
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    }
  }, [t, show])
  const over = useFileDrop(open && !busy, IMAGE_EXT, (p) => void choose(p))

  const reset = () => {
    setPaths([])
    setPreview(null)
    setText('')
    setResult(null)
    setFrom(null)
    setPrint(false)
  }
  const loadTxt = async () => {
    const f = await pickFile('Text', ['txt'])
    if (!f) return
    try {
      setText(await api.readTextFile(f))
      setFrom(fileName(f))
      setResult(null)
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    }
  }
  const add = async () => {
    if (!paths.length) return
    setBusy(true)
    try {
      const r = await runJob<Report>(() => api.importHandwritingPhoto(hand, paths, text, 'A4', print))
      setResult(r)
      if (r.ok) setPages((n) => n + 1)
      onImported()
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    } finally {
      setBusy(false)
    }
  }
  const lines = text.split('\n').filter((l) => l.trim()).length
  const unmatched = (result?.warnings ?? []).filter((w) => w.startsWith('could not align: ')).map((w) => w.slice('could not align: '.length))
  const done = !!result?.ok
  return (
    <Dialog open={open} onOpenChange={(v) => { onOpenChange(v); if (!v) { reset(); setPages(0) } }} title={t('hands.photo.title')} description={t('hands.photo.body')} width="max-w-5xl"
      footer={
        <>
          {pages > 0 && <span className="mr-auto self-center text-[0.82rem] text-muted">{t('hands.photo.pages', { n: pages })}</span>}
          <Button variant="quiet" onClick={() => onOpenChange(false)}>{t('common.close')}</Button>
          {done
            ? <Button variant="primary" onClick={reset}><ImagePlus />{t('hands.photo.another')}</Button>
            : <Button variant="primary" disabled={!paths.length || !text.trim() || busy} onClick={add}>{busy ? t('common.working') : t('hands.photo.add')}</Button>}
        </>
      }
    >
      <div className="grid grid-cols-[minmax(0,1fr)_22rem] gap-4 pb-2">
        <div className="relative h-[30rem] overflow-hidden rounded-lg border border-line">
          {paths.length > 1 && (
            <div className="absolute left-2 top-2 z-10 flex gap-1" role="group" aria-label={t('hands.photo.photos', { n: paths.length })}>
              {paths.map((p, i) => (
                <Button key={p} size="sm" variant={i === shown ? 'primary' : 'secondary'} aria-pressed={i === shown} title={fileName(p)} onClick={() => { setShown(i); void show(p) }}>{i + 1}</Button>
              ))}
            </div>
          )}
          {preview ? (
            <ZoomPan width={preview.w} height={preview.h} fitKey={preview.url.length} label={t('hands.photo.title')} padding={8}>
              <img src={preview.url} width={preview.w} height={preview.h} alt="" draggable={false} className="block select-none" />
            </ZoomPan>
          ) : (
            <button onClick={async () => choose(await pickImages(true))} disabled={busy}
              className={cn('flex h-full w-full flex-col items-center justify-center gap-2 text-muted hover:text-graphite', over && 'bg-ink-soft/50 text-ink')}>
              <ImagePlus className="size-6" />
              {paths.length ? t('common.loading') : t('hands.photo.drop')}
            </button>
          )}
        </div>
        <div className="flex min-w-0 flex-col gap-2">
          <div className="flex items-center gap-2">
            <Label htmlFor="transcript" className="flex-1">{t('hands.photo.transcript')}</Label>
            <span className="text-[0.78rem] tabular-nums text-muted">{t('hands.photo.lines', { n: lines })}</span>
            <Button size="sm" variant="quiet" onClick={() => void loadTxt()} disabled={busy}><FileText />{t('hands.photo.loadTxt')}</Button>
          </div>
          <div className="flex items-center gap-2">
            <span className="flex-1 text-[0.8rem] text-muted">{t('hands.photo.style')}</span>
            <Segmented<'joined' | 'print'> label={t('hands.photo.style')} value={print ? 'print' : 'joined'} onChange={(v) => { setPrint(v === 'print'); setResult(null) }} options={[
              { value: 'joined', label: t('hands.photo.joined') },
              { value: 'print', label: t('hands.photo.print') },
            ]} />
          </div>
          <Textarea id="transcript" rows={12} value={text} onChange={(e) => { setText(e.target.value); setResult(null); setFrom(null) }} spellCheck={false} className="flex-1 font-[inherit] leading-relaxed" disabled={busy} />
          {from && <p className="text-[0.78rem] text-muted">{t('hands.photo.fromFile', { name: from })}</p>}
          {given && <p className="rounded-md bg-ink-soft/50 px-2 py-1.5 text-[0.8rem] text-graphite">{t('hands.photo.givenHint')}</p>}
          {paths.length > 0 && <Button size="sm" variant="quiet" className="self-start" onClick={async () => choose(await pickImages(true))}><ImagePlus />{t('hands.photo.change')}</Button>}
          {result ? (
            <div className="space-y-1 text-[0.85rem]" role="status">
              {result.ok
                ? <p className="flex items-center gap-1.5 text-ok"><Check className="size-4" />{t('hands.photo.added', { n: result.glyphs })}</p>
                : <p className="flex items-center gap-1.5 text-redpen"><TriangleAlert className="size-4" />{result.error}</p>}
              {unmatched.length > 0 && <p className="text-muted">{t('hands.photo.unmatched', { words: unmatched.join(', ') })}</p>}
              {result.ok && result.px_mm > 0 && result.px_mm < SHARP_PX_MM && <p className="text-redpen">{t('hands.photo.small')}</p>}
            </div>
          ) : (
            <ul className="list-disc space-y-1 pl-4 text-[0.8rem] text-muted">
              <li>{t('hands.photo.hint1')}</li>
              <li>{t('hands.photo.hint2')}</li>
              <li>{t('hands.photo.hint3')}</li>
            </ul>
          )}
        </div>
      </div>
    </Dialog>
  )
}
