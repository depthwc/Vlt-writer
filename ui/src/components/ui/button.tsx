import { cva, type VariantProps } from 'class-variance-authority'
import { Slot } from 'radix-ui'
import * as React from 'react'

import { cn } from '@/lib/utils'

const button = cva(
  'inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-[var(--radius-ctl)] font-medium transition-colors disabled:pointer-events-none disabled:opacity-45 [&_svg]:size-4 [&_svg]:shrink-0',
  {
    variants: {
      variant: {
        primary: 'bg-ink text-on-ink hover:bg-ink-hover',
        secondary: 'border border-line bg-surface text-graphite hover:border-ink/60 hover:bg-ink-soft/40',
        ghost: 'text-graphite hover:bg-ink-soft/60',
        danger: 'border border-redpen/40 bg-surface text-redpen hover:bg-redpen-soft',
        quiet: 'text-muted hover:text-graphite hover:bg-paper-2',
      },
      size: {
        sm: 'h-7 px-2.5 text-[0.8rem]',
        md: 'h-8 px-3 text-[0.85rem]',
        lg: 'h-10 px-4 text-[0.95rem]',
        icon: 'size-8',
        'icon-sm': 'size-7',
      },
    },
    defaultVariants: { variant: 'secondary', size: 'md' },
  },
)

export interface ButtonProps extends React.ComponentProps<'button'>, VariantProps<typeof button> {
  asChild?: boolean
}

export function Button({ className, variant, size, asChild, ...props }: ButtonProps) {
  const Comp = asChild ? Slot.Root : 'button'
  return <Comp className={cn(button({ variant, size }), className)} {...props} />
}
