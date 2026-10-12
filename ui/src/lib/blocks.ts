import type { Block } from './api'

/** Minimal Tiptap/ProseMirror JSON shape. */
export interface PMNode {
  type: string
  attrs?: Record<string, unknown>
  content?: PMNode[]
  text?: string
  marks?: { type: string }[]
}

const SUP = '\u0001'
const SUB = '\u0002'
const END = '\u0003'
const BOLD = '\u0004'
const BOLD_END = '\u0005'
const UNDER = '\u0006'
const UNDER_END = '\u0007'
/** A new line inside a paragraph (Shift+Enter). */
const LINE_BREAK = '\u2028'
/** An empty paragraph: an empty line on the page. */
const BLANK_LINE = '\u000e'

// ---------------------------------------------------------------- editor -> markdown-lite (parsed by Rust)
function inline(nodes: PMNode[] = []): string {
  let out = ''
  for (const n of nodes) {
    if (n.type === 'text') {
      let t = n.text ?? ''
      const marks = new Set((n.marks ?? []).map((m) => m.type))
      if (marks.has('superscript')) t = `$^{${t}}$`
      else if (marks.has('subscript')) t = `$_{${t}}$`
      if (marks.has('bold') && t.trim()) t = `**${t}**`
      if (marks.has('underline') && t.trim()) t = `<u>${t}</u>`
      out += t
    } else if (n.type === 'inlineMath') {
      out += `$${String(n.attrs?.latex ?? '')}$`
    } else if (n.type === 'hardBreak') {
      out += LINE_BREAK
    } else if (n.content) {
      out += inline(n.content)
    }
  }
  return out
}

function listItems(list: PMNode, ordered: boolean, depth: number, out: string[]) {
  let i = Number(list.attrs?.start ?? 1)
  for (const item of list.content ?? []) {
    const pad = '  '.repeat(depth)
    const first = (item.content ?? []).find((c) => c.type === 'paragraph')
    out.push(`${pad}${ordered ? `${i}.` : '-'} ${inline(first?.content)}`.trimEnd())
    i += 1
    for (const c of item.content ?? []) {
      if (c.type === 'bulletList') listItems(c, false, depth + 1, out)
      if (c.type === 'orderedList') listItems(c, true, depth + 1, out)
    }
  }
}

function cellText(cell: PMNode): string {
  return (cell.content ?? []).map((p) => inline(p.content)).join(' ').replace(/\|/g, '/').trim()
}

export function docToSource(doc: PMNode | null | undefined): string {
  const parts: string[] = []
  for (const n of doc?.content ?? []) {
    switch (n.type) {
      case 'heading':
        parts.push(`${'#'.repeat(Number(n.attrs?.level ?? 1))} ${inline(n.content)}`)
        break
      case 'paragraph': {
        // spaces count; an empty paragraph is an empty line
        const t = inline(n.content)
        parts.push(t.trim() ? t.trimEnd() : BLANK_LINE)
        break
      }
      case 'bulletList':
      case 'orderedList': {
        const items: string[] = []
        listItems(n, n.type === 'orderedList', 0, items)
        parts.push(items.join('\n'))
        break
      }
      case 'table':
        parts.push((n.content ?? []).map((row) => `| ${(row.content ?? []).map(cellText).join(' | ')} |`).join('\n'))
        break
      case 'horizontalRule':
        parts.push('---')
        break
      case 'blockMath':
        parts.push(`$$${String(n.attrs?.latex ?? '')}$$`)
        break
      case 'pageBreak':
        parts.push('\f')
        break
      case 'blockquote':
        parts.push(docToSource(n))
        break
    }
  }
  return parts.join('\n\n')
}

// ---------------------------------------------------------------- engine blocks -> editor
function textNodes(s: string): PMNode[] {
  const out: PMNode[] = []
  let buf = ''
  let mode: 'sup' | 'sub' | null = null
  let bold = false
  let under = false
  const flush = () => {
    if (!buf) return
    const marks: { type: string }[] = []
    if (mode === 'sup') marks.push({ type: 'superscript' })
    if (mode === 'sub') marks.push({ type: 'subscript' })
    if (bold) marks.push({ type: 'bold' })
    if (under) marks.push({ type: 'underline' })
    out.push(marks.length ? { type: 'text', text: buf, marks } : { type: 'text', text: buf })
    buf = ''
  }
  for (const ch of s) {
    if (ch === SUP || ch === SUB) {
      flush()
      mode = ch === SUP ? 'sup' : 'sub'
    } else if (ch === END) {
      flush()
      mode = null
    } else if (ch === BOLD || ch === BOLD_END) {
      flush()
      bold = ch === BOLD
    } else if (ch === UNDER || ch === UNDER_END) {
      flush()
      under = ch === UNDER
    } else if (ch === LINE_BREAK) {
      flush()
      out.push({ type: 'hardBreak' })
    } else {
      buf += ch
    }
  }
  flush()
  return out
}

const para = (text: string): PMNode => ({ type: 'paragraph', content: textNodes(text) })

export function blocksToDoc(blocks: Block[]): PMNode {
  const content: PMNode[] = []
  let list: PMNode | null = null
  for (const b of blocks) {
    if (b.t !== 'li') list = null
    switch (b.t) {
      case 'h':
        content.push({ type: 'heading', attrs: { level: Math.min(3, Math.max(1, b.level)) }, content: textNodes(b.text) })
        break
      case 'p':
        content.push(para(b.text))
        break
      case 'li': {
        const ordered = b.marker !== '-'
        const type = ordered ? 'orderedList' : 'bulletList'
        if (!list || list.type !== type) {
          list = { type, attrs: ordered ? { start: parseInt(b.marker, 10) || 1 } : undefined, content: [] }
          content.push(list)
        }
        list.content!.push({ type: 'listItem', content: [para(b.text)] })
        break
      }
      case 'table': {
        const cols = Math.max(1, ...b.rows.map((r) => r.length))
        content.push({
          type: 'table',
          content: b.rows.map((r, ri) => ({
            type: 'tableRow',
            content: Array.from({ length: cols }, (_, ci) => ({ type: ri === 0 ? 'tableHeader' : 'tableCell', content: [para(r[ci] ?? '')] })),
          })),
        })
        break
      }
      case 'math':
        content.push({ type: 'blockMath', attrs: { latex: b.tex } })
        break
      case 'rule':
        content.push({ type: 'horizontalRule' })
        break
      case 'br':
        content.push({ type: 'pageBreak' })
        break
    }
  }
  if (!content.length) content.push({ type: 'paragraph' })
  return { type: 'doc', content }
}

export function plainToDoc(text: string): PMNode {
  return { type: 'doc', content: text.split(/\n{2,}/).map((p) => ({ type: 'paragraph', content: p.trim() ? [{ type: 'text', text: p.replace(/\n/g, ' ') }] : [] })) }
}
