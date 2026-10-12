import { FolderOpen, HardDriveDownload } from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import { Kbd, Segmented, Slider, Switch } from '@/components/ui/controls'
import { LANGS } from '@/i18n'
import { api, inTauri } from '@/lib/api'
import { saveAs } from '@/lib/files'
import { useApp, type Theme } from '@/state/app'

const SHORTCUTS: [string, string][] = [
  ['Ctrl+K', 'nav.palette'],
  ['Ctrl+N', 'write.newPage'],
  ['Ctrl+O', 'write.import'],
  ['Ctrl+E', 'write.exportBtn'],
  ['Ctrl+1', 'nav.write'],
  ['Ctrl+2', 'nav.hands'],
  ['Ctrl+3', 'nav.papers'],
  ['Ctrl+,', 'nav.settings'],
  ['+ / − / 0', 'common.fit'],
]

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="grid grid-cols-[12rem_1fr] gap-6 border-b border-line py-5 last:border-0">
      <h2 className="pt-1 text-[0.9rem] font-semibold">{title}</h2>
      <div className="min-w-0">{children}</div>
    </section>
  )
}

export default function SettingsWorkspace() {
  const { t } = useTranslation()
  const s = useApp()
  const [lib, setLib] = React.useState('')
  React.useEffect(() => {
    if (inTauri()) void api.libraryPath().then(setLib)
  }, [])

  const backup = async () => {
    const path = await saveAs(`Vlt-writer-${new Date().toISOString().slice(0, 10)}.zip`, 'Zip', ['zip'])
    if (!path) return
    try {
      await api.backupLibrary(path)
      toast.success(t('settings.backupDone', { path }))
    } catch (e) {
      toast.error(t('errors.generic', { msg: String(e) }))
    }
  }

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-3xl px-8 py-6">
        <h1 className="mb-2 text-[1.35rem] font-semibold">{t('settings.title')}</h1>
        <Section title={t('settings.language')}>
          <Segmented label={t('settings.language')} value={s.lang} onChange={s.setLang} options={LANGS.map((l) => ({ value: l.code, label: l.label }))} />
        </Section>
        <Section title={t('settings.theme')}>
          <Segmented<Theme>
            label={t('settings.theme')}
            value={s.theme}
            onChange={s.setTheme}
            options={(['light', 'dark'] as Theme[]).map((v) => ({ value: v, label: t(`settings.themes.${v}`) }))}
          />
        </Section>
        <Section title={t('settings.scale')}>
          <div className="max-w-sm">
            <Slider label={t('settings.scale')} value={s.scale} min={0.85} max={1.4} step={0.05} onChange={s.setScale} format={(v) => `${Math.round(v * 100)}%`} />
          </div>
          <label className="mt-4 flex items-center gap-3">
            <Switch checked={s.reducedMotion} onCheckedChange={s.setReducedMotion} label={t('settings.reducedMotion')} />
            {t('settings.reducedMotion')}
          </label>
        </Section>
        <Section title={t('settings.library')}>
          <p className="mb-3 break-all text-muted">{lib}</p>
          <div className="flex gap-2">
            <Button onClick={() => lib && api.openFolder(lib).catch((e) => toast.error(t('errors.generic', { msg: String(e) })))}><FolderOpen />{t('common.open')}</Button>
            <Button onClick={backup}><HardDriveDownload />{t('settings.backup')}</Button>
          </div>
        </Section>
        <Section title={t('settings.shortcuts')}>
          <dl className="grid grid-cols-[8rem_1fr] gap-y-2">
            {SHORTCUTS.map(([k, label]) => (
              <React.Fragment key={k}>
                <dt><Kbd>{k}</Kbd></dt>
                <dd className="text-muted">{t(label)}</dd>
              </React.Fragment>
            ))}
          </dl>
          <Button className="mt-4" variant="quiet" onClick={() => s.setTourOpen(true)}>{t('settings.tour')}</Button>
        </Section>
        <p className="py-6 text-muted">{t('settings.about')}</p>
      </div>
    </div>
  )
}
