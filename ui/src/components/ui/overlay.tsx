import { Dialog as DialogP, DropdownMenu as MenuP, Popover as PopoverP, Tooltip as TooltipP } from 'radix-ui'
import { X } from 'lucide-react'
import * as React from 'react'

import { cn } from '@/lib/utils'

export const TooltipProvider = TooltipP.Provider

export function Tip({ label, children, side = 'bottom', shortcut }: { label: string; children: React.ReactElement; side?: 'top' | 'bottom' | 'left' | 'right'; shortcut?: string }) {
  return (
    <TooltipP.Root delayDuration={350}>
      <TooltipP.Trigger asChild>{children}</TooltipP.Trigger>
      <TooltipP.Portal>
        <TooltipP.Content side={side} sideOffset={6} className="z-50 flex items-center gap-2 rounded-md bg-graphite px-2 py-1 text-[0.75rem] text-paper shadow-pop">
          {label}
          {shortcut && <span className="opacity-60">{shortcut}</span>}
        </TooltipP.Content>
      </TooltipP.Portal>
    </TooltipP.Root>
  )
}

export function Dialog({ open, onOpenChange, title, description, children, footer, width = 'max-w-lg' }: {
  open: boolean
  onOpenChange: (v: boolean) => void
  title: string
  description?: string
  children: React.ReactNode
  footer?: React.ReactNode
  width?: string
}) {
  return (
    <DialogP.Root open={open} onOpenChange={onOpenChange}>
      <DialogP.Portal>
        <DialogP.Overlay className="fixed inset-0 z-40 bg-graphite/30" />
        <DialogP.Content className={cn('fixed left-1/2 top-1/2 z-50 flex max-h-[88vh] w-[92vw] -translate-x-1/2 -translate-y-1/2 flex-col rounded-xl border border-line bg-surface shadow-pop', width)}>
          <div className="flex items-start justify-between gap-4 px-5 pb-2 pt-4">
            <div>
              <DialogP.Title className="text-[1.05rem] font-semibold">{title}</DialogP.Title>
              {description ? (
                <DialogP.Description className="mt-0.5 text-[0.85rem] text-muted">{description}</DialogP.Description>
              ) : (
                <DialogP.Description className="sr-only">{title}</DialogP.Description>
              )}
            </div>
            <DialogP.Close className="rounded-md p-1 text-muted hover:bg-paper-2 hover:text-graphite" aria-label="Close">
              <X className="size-4" />
            </DialogP.Close>
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-5 py-2">{children}</div>
          {footer && <div className="flex justify-end gap-2 border-t border-line px-5 py-3">{footer}</div>}
        </DialogP.Content>
      </DialogP.Portal>
    </DialogP.Root>
  )
}

export const Menu = MenuP.Root
export const MenuTrigger = MenuP.Trigger

export function MenuContent({ children, align = 'start' }: { children: React.ReactNode; align?: 'start' | 'end' | 'center' }) {
  return (
    <MenuP.Portal>
      <MenuP.Content align={align} sideOffset={4} className="z-50 min-w-48 rounded-lg border border-line bg-surface p-1 shadow-pop">
        {children}
      </MenuP.Content>
    </MenuP.Portal>
  )
}

export function MenuItem({ children, onSelect, danger, shortcut, disabled }: { children: React.ReactNode; onSelect: () => void; danger?: boolean; shortcut?: string; disabled?: boolean }) {
  return (
    <MenuP.Item
      disabled={disabled}
      onSelect={onSelect}
      className={cn('flex h-8 cursor-default select-none items-center gap-2 rounded-md px-2 text-[0.85rem] outline-none data-[disabled]:opacity-40 data-[highlighted]:bg-ink-soft [&_svg]:size-4 [&_svg]:text-muted', danger && 'text-redpen data-[highlighted]:bg-redpen-soft [&_svg]:text-redpen')}
    >
      <span className="flex flex-1 items-center gap-2">{children}</span>
      {shortcut && <span className="text-[0.75rem] text-muted">{shortcut}</span>}
    </MenuP.Item>
  )
}

export function MenuSeparator() {
  return <MenuP.Separator className="my-1 h-px bg-line" />
}

export function Popover({ trigger, children, align = 'start' }: { trigger: React.ReactElement; children: React.ReactNode; align?: 'start' | 'end' | 'center' }) {
  return (
    <PopoverP.Root>
      <PopoverP.Trigger asChild>{trigger}</PopoverP.Trigger>
      <PopoverP.Portal>
        <PopoverP.Content align={align} sideOffset={6} className="z-50 rounded-lg border border-line bg-surface p-3 shadow-pop outline-none">
          {children}
        </PopoverP.Content>
      </PopoverP.Portal>
    </PopoverP.Root>
  )
}
