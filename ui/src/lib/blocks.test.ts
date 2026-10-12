import { describe, expect, it } from 'vitest'

import type { Block } from './api'
import { blocksToDoc, docToSource, type PMNode } from './blocks'

describe('editor <-> engine', () => {
  it('writes markdown-lite the Rust parser understands', () => {
    const doc: PMNode = {
      type: 'doc',
      content: [
        { type: 'heading', attrs: { level: 1 }, content: [{ type: 'text', text: 'Uy vazifasi' }] },
        { type: 'paragraph', content: [
          { type: 'text', text: 'x' },
          { type: 'text', text: '2', marks: [{ type: 'superscript' }] },
          { type: 'text', text: ' and ' },
          { type: 'text', text: 'bold', marks: [{ type: 'bold' }] },
          { type: 'text', text: ' ' },
          { type: 'inlineMath', attrs: { latex: '\\frac{a}{b}' } },
        ] },
        { type: 'bulletList', content: [
          { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'one' }] }] },
          { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'two' }] }] },
        ] },
        { type: 'table', content: [
          { type: 'tableRow', content: [
            { type: 'tableHeader', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Ism' }] }] },
            { type: 'tableHeader', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Baho' }] }] },
          ] },
          { type: 'tableRow', content: [
            { type: 'tableCell', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Ali' }] }] },
            { type: 'tableCell', content: [{ type: 'paragraph', content: [{ type: 'text', text: '5' }] }] },
          ] },
        ] },
        { type: 'horizontalRule' },
        { type: 'blockMath', attrs: { latex: 'E=mc^2' } },
        { type: 'pageBreak' },
      ],
    }
    expect(docToSource(doc)).toBe(
      '# Uy vazifasi\n\nx$^{2}$ and **bold** $\\frac{a}{b}$\n\n- one\n- two\n\n| Ism | Baho |\n| Ali | 5 |\n\n---\n\n$$E=mc^2$$\n\n\f',
    )
  })

  it('turns imported blocks into editor nodes', () => {
    const blocks: Block[] = [
      { t: 'h', text: 'Title', level: 1 },
      { t: 'p', text: 'x\u00012\u0003 \u0004b\u0005' },
      { t: 'li', text: 'a', marker: '1.' },
      { t: 'li', text: 'b', marker: '2.' },
      { t: 'table', rows: [['A', 'B'], ['C']] },
      { t: 'rule' },
    ]
    const doc = blocksToDoc(blocks)
    expect(doc.content!.map((n) => n.type)).toEqual(['heading', 'paragraph', 'orderedList', 'table', 'horizontalRule'])
    const p = doc.content![1].content!
    expect(p[1]).toEqual({ type: 'text', text: '2', marks: [{ type: 'superscript' }] })
    expect(p[3]).toEqual({ type: 'text', text: 'b', marks: [{ type: 'bold' }] })
    expect(doc.content![3].content![1].content!.length).toBe(2)
  })
})
