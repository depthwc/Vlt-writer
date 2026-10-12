import { Checkbox as CheckboxP, Label as LabelP, Progress as ProgressP, Select as SelectP, Slider as SliderP, Switch as SwitchP, ToggleGroup as ToggleGroupP } from 'radix-ui'
import { Check, ChevronDown } from 'lucide-react'
import * as React from 'react'

import { cn } from '@/lib/utils'

const field = 'h-8 w-full rounded-[var(--radius-ctl)] border border-line bg-surface px-2.5 text-[0.85rem] text-graphite placeholder:text-muted/80 hover:border-ink/40 focus-visible:border-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/25'

export function Input({ className, ...p }: React.ComponentProps<'input'>) {
  return <input className={cn(field, className)} {...p} />
}

/** A number typed freely and kept on Enter or on leaving the field (Escape puts it back). */
export function NumberField({ value, onCommit, digits = 1, min, max, className, ...p }: Omit<React.ComponentProps<'input'>, 'value' | 'onChange' | 'min' | 'max'> & {
  value: number
  onCommit: (v: number) => void
  digits?: number
  min?: number
  max?: number
}) {
  const shown = value.toFixed(digits)
  const [draft, setDraft] = React.useState<string | null>(null)
  const commit = () => {
    if (draft === null) return
    const v = Number(draft.replace(',', '.'))
    setDraft(null)
    if (draft.trim() === '' || !Number.isFinite(v)) return
    const c = Math.min(max ?? Infinity, Math.max(min ?? -Infinity, v))
    if (c.toFixed(digits) !== shown) onCommit(c)
  }
  return (
    <input type="text" inputMode="decimal" className={cn(field, 'tabular-nums', className)} {...p}
      value={draft ?? shown}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') commit()
        else if (e.key === 'Escape') setDraft(null)
      }} />
  )
}

export function Textarea({ className, ...p }: React.ComponentProps<'textarea'>) {
  return <textarea className={cn(field, 'h-auto min-h-20 py-2 leading-snug', className)} {...p} />
}

export function Label({ className, ...p }: React.ComponentProps<typeof LabelP.Root>) {
  return <LabelP.Root className={cn('text-[0.8rem] text-muted', className)} {...p} />
}

/** Label on the left, control on the right: the inspector rhythm. */
export function Row({ label, htmlFor, children, hint, stack }: { label: string; htmlFor?: string; children: React.ReactNode; hint?: string; stack?: boolean }) {
  return (
    <div className={stack ? 'flex flex-col gap-1 py-1' : 'grid grid-cols-[7.5rem_1fr] items-center gap-2 py-1'}>
      <Label htmlFor={htmlFor} title={hint}>
        {label}
      </Label>
      <div className="min-w-0">{children}</div>
    </div>
  )
}

export function Select({ value, onValueChange, options, id, label, className, placeholder = '—' }: {
  value: string
  onValueChange: (v: string) => void
  options: { value: string; label: string }[]
  id?: string
  label?: string
  className?: string
  placeholder?: string
}) {
  return (
    <SelectP.Root value={value || undefined} onValueChange={onValueChange}>
      <SelectP.Trigger id={id} aria-label={label} className={cn(field, 'flex items-center justify-between gap-2 text-left data-[placeholder]:text-muted', className)}>
        <span className="truncate"><SelectP.Value placeholder={placeholder} /></span>
        <SelectP.Icon>
          <ChevronDown className="size-4 text-muted" />
        </SelectP.Icon>
      </SelectP.Trigger>
      <SelectP.Portal>
        <SelectP.Content position="popper" sideOffset={4} className="z-50 max-h-80 min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-lg border border-line bg-surface shadow-pop">
          <SelectP.Viewport className="p-1">
            {options.map((o) => (
              <SelectP.Item key={o.value} value={o.value} className="relative flex h-8 cursor-default select-none items-center rounded-md pl-7 pr-2 text-[0.85rem] outline-none data-[highlighted]:bg-ink-soft data-[state=checked]:font-medium">
                <SelectP.ItemIndicator className="absolute left-2">
                  <Check className="size-3.5 text-ink" />
                </SelectP.ItemIndicator>
                <SelectP.ItemText>{o.label}</SelectP.ItemText>
              </SelectP.Item>
            ))}
          </SelectP.Viewport>
        </SelectP.Content>
      </SelectP.Portal>
    </SelectP.Root>
  )
}

export function Slider({ value, onChange, min, max, step, label, format, id }: {
  value: number
  onChange: (v: number) => void
  min: number
  max: number
  step: number
  label: string
  format?: (v: number) => string
  id?: string
}) {
  return (
    <div className="flex items-center gap-2.5">
      <SliderP.Root id={id} className="relative flex h-5 flex-1 touch-none select-none items-center" value={[value]} min={min} max={max} step={step} onValueChange={(v) => onChange(v[0])} aria-label={label}>
        <SliderP.Track className="relative h-1 grow rounded-full bg-grid">
          <SliderP.Range className="absolute h-full rounded-full bg-ink" />
        </SliderP.Track>
        <SliderP.Thumb className="block size-3.5 rounded-full border-2 border-ink bg-surface shadow focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink" aria-label={label} />
      </SliderP.Root>
      <span className="min-w-10 whitespace-nowrap text-right tabular-nums text-[0.8rem] text-muted">{format ? format(value) : value}</span>
    </div>
  )
}

export function Switch({ checked, onCheckedChange, id, label }: { checked: boolean; onCheckedChange: (v: boolean) => void; id?: string; label?: string }) {
  return (
    <SwitchP.Root id={id} aria-label={label} checked={checked} onCheckedChange={onCheckedChange} className="relative h-5 w-9 shrink-0 rounded-full bg-grid transition-colors data-[state=checked]:bg-ink">
      <SwitchP.Thumb className="block size-4 translate-x-0.5 rounded-full bg-surface shadow transition-transform data-[state=checked]:translate-x-[1.1rem]" />
    </SwitchP.Root>
  )
}

export function Checkbox({ checked, onCheckedChange, id, label }: { checked: boolean; onCheckedChange: (v: boolean) => void; id?: string; label?: string }) {
  return (
    <CheckboxP.Root id={id} aria-label={label} checked={checked} onCheckedChange={(v) => onCheckedChange(v === true)} className="flex size-4 shrink-0 items-center justify-center rounded border border-line bg-surface data-[state=checked]:border-ink data-[state=checked]:bg-ink">
      <CheckboxP.Indicator>
        <Check className="size-3 text-on-ink" strokeWidth={3} />
      </CheckboxP.Indicator>
    </CheckboxP.Root>
  )
}

/** Segmented control. */
export function Segmented<T extends string>({ value, onChange, options, label, className }: {
  value: T
  onChange: (v: T) => void
  options: { value: T; label: React.ReactNode; title?: string }[]
  label: string
  className?: string
}) {
  return (
    <ToggleGroupP.Root
      type="single"
      value={value}
      onValueChange={(v) => v && onChange(v as T)}
      aria-label={label}
      className={cn('inline-flex h-8 items-center rounded-[var(--radius-ctl)] border border-line bg-paper-2 p-0.5', className)}
    >
      {options.map((o) => (
        <ToggleGroupP.Item
          key={o.value}
          value={o.value}
          title={o.title}
          aria-label={o.title}
          className="inline-flex h-full flex-1 items-center justify-center gap-1 whitespace-nowrap rounded-[5px] px-2.5 text-[0.8rem] text-muted hover:text-graphite data-[state=on]:bg-surface data-[state=on]:text-graphite data-[state=on]:shadow-sm [&_svg]:size-4"
        >
          {o.label}
        </ToggleGroupP.Item>
      ))}
    </ToggleGroupP.Root>
  )
}

export function Progress({ value, label }: { value: number; label: string }) {
  return (
    <ProgressP.Root value={value} aria-label={label} className="relative h-1.5 w-full overflow-hidden rounded-full bg-grid">
      <ProgressP.Indicator className="h-full bg-ink transition-[width]" style={{ width: `${value}%` }} />
    </ProgressP.Root>
  )
}

export function Kbd({ children }: { children: React.ReactNode }) {
  return <kbd className="rounded border border-line bg-paper-2 px-1.5 py-px font-sans text-[0.72rem] text-muted">{children}</kbd>
}
