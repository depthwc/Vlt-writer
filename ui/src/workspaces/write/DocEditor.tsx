import Mathematics from '@tiptap/extension-mathematics'
import Subscript from '@tiptap/extension-subscript'
import Superscript from '@tiptap/extension-superscript'
import { Table, TableCell, TableHeader, TableRow } from '@tiptap/extension-table'
import { Placeholder } from '@tiptap/extensions'
import { EditorContent, Node, mergeAttributes, useEditor, useEditorState, type Editor } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import {
  Bold, Columns3, Heading1, Heading2, List, ListOrdered, Minus, Rows3, Sigma, SeparatorHorizontal, Subscript as SubIcon,
  Superscript as SupIcon, Table2, Trash2, Underline as UnderlineIcon, Undo2, Redo2,
} from 'lucide-react'
import * as React from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/controls'
import { Dialog, Tip } from '@/components/ui/overlay'
import { docToSource, type PMNode } from '@/lib/blocks'
import { cn } from '@/lib/utils'

/** A page break the engine turns into a new sheet. */
const PageBreak = Node.create({
  name: 'pageBreak',
  group: 'block',
  atom: true,
  parseHTML: () => [{ tag: 'hr[data-page-break]' }],
  renderHTML: ({ HTMLAttributes }) => ['hr', mergeAttributes(HTMLAttributes, { 'data-page-break': '', class: 'page-break' })],
  addCommands() {
    return {
      setPageBreak: () => ({ commands }: { commands: { insertContent: (c: unknown) => boolean } }) => commands.insertContent({ type: this.name }),
    } as never
  },
})

function MathDialog({ open, initial, onClose, onSubmit }: { open: boolean; initial: string; onClose: () => void; onSubmit: (latex: string) => void }) {
  const { t } = useTranslation()
  const [v, setV] = React.useState(initial)
  React.useEffect(() => setV(initial), [initial, open])
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()} title={t('editor.math')} description={t('editor.mathPrompt')} width="max-w-md"
      footer={<><Button variant="quiet" onClick={onClose}>{t('common.cancel')}</Button><Button variant="primary" disabled={!v.trim()} onClick={() => onSubmit(v.trim())}>{t('common.apply')}</Button></>}>
      <form className="pb-2" onSubmit={(e) => { e.preventDefault(); if (v.trim()) onSubmit(v.trim()) }}>
        <Input autoFocus value={v} onChange={(e) => setV(e.target.value)} aria-label={t('editor.math')} className="font-[ui-monospace,Consolas,monospace]" />
      </form>
    </Dialog>
  )
}

function Tool({ label, onClick, active, children, shortcut, disabled }: { label: string; onClick: () => void; active?: boolean; children: React.ReactNode; shortcut?: string; disabled?: boolean }) {
  return (
    <Tip label={label} shortcut={shortcut}>
      <Button variant="ghost" size="icon-sm" aria-label={label} aria-pressed={active} disabled={disabled} onMouseDown={(e) => e.preventDefault()} onClick={onClick}
        className={cn(active && 'bg-ink-soft text-ink')}>
        {children}
      </Button>
    </Tip>
  )
}

function Toolbar({ editor, onMath }: { editor: Editor; onMath: () => void }) {
  const { t } = useTranslation()
  const s = useEditorState({
    editor,
    selector: ({ editor: e }) => ({
      h1: e.isActive('heading', { level: 1 }),
      h2: e.isActive('heading', { level: 2 }),
      bold: e.isActive('bold'),
      under: e.isActive('underline'),
      sup: e.isActive('superscript'),
      sub: e.isActive('subscript'),
      bullet: e.isActive('bulletList'),
      ordered: e.isActive('orderedList'),
      table: e.isActive('table'),
      undo: e.can().undo(),
      redo: e.can().redo(),
    }),
  })
  const c = () => editor.chain().focus()
  const sep = <span className="mx-1 h-5 w-px bg-line" aria-hidden />
  return (
    <div role="toolbar" aria-label="Formatting" className="flex flex-wrap items-center gap-0.5 border-b border-line bg-surface px-2 py-1">
      <Tool label={t('common.undo')} shortcut="Ctrl+Z" onClick={() => c().undo().run()} disabled={!s.undo}><Undo2 /></Tool>
      <Tool label={t('common.redo')} shortcut="Ctrl+Y" onClick={() => c().redo().run()} disabled={!s.redo}><Redo2 /></Tool>
      {sep}
      <Tool label={t('editor.h1')} active={s.h1} onClick={() => c().toggleHeading({ level: 1 }).run()}><Heading1 /></Tool>
      <Tool label={t('editor.h2')} active={s.h2} onClick={() => c().toggleHeading({ level: 2 }).run()}><Heading2 /></Tool>
      <Tool label={t('editor.bold')} shortcut="Ctrl+B" active={s.bold} onClick={() => c().toggleBold().run()}><Bold /></Tool>
      <Tool label={t('editor.underline')} shortcut="Ctrl+U" active={s.under} onClick={() => c().toggleUnderline().run()}><UnderlineIcon /></Tool>
      <Tool label={t('editor.sup')} active={s.sup} onClick={() => c().toggleSuperscript().run()}><SupIcon /></Tool>
      <Tool label={t('editor.sub')} active={s.sub} onClick={() => c().toggleSubscript().run()}><SubIcon /></Tool>
      {sep}
      <Tool label={t('editor.bullet')} active={s.bullet} onClick={() => c().toggleBulletList().run()}><List /></Tool>
      <Tool label={t('editor.ordered')} active={s.ordered} onClick={() => c().toggleOrderedList().run()}><ListOrdered /></Tool>
      <Tool label={t('editor.table')} onClick={() => c().insertTable({ rows: 3, cols: 2, withHeaderRow: true }).run()}><Table2 /></Tool>
      <Tool label={t('editor.math')} onClick={onMath}><Sigma /></Tool>
      <Tool label={t('editor.rule')} onClick={() => c().setHorizontalRule().run()}><Minus /></Tool>
      <Tool label={t('editor.pageBreak')} onClick={() => (c() as unknown as { setPageBreak: () => { run: () => void } }).setPageBreak().run()}><SeparatorHorizontal /></Tool>
      {s.table && (
        <>
          {sep}
          <Tool label={t('editor.addRow')} onClick={() => c().addRowAfter().run()}><Rows3 /></Tool>
          <Tool label={t('editor.addCol')} onClick={() => c().addColumnAfter().run()}><Columns3 /></Tool>
          <Tool label={t('editor.delRow')} onClick={() => c().deleteRow().run()}><Rows3 className="text-redpen" /></Tool>
          <Tool label={t('editor.delCol')} onClick={() => c().deleteColumn().run()}><Columns3 className="text-redpen" /></Tool>
          <Tool label={t('editor.delTable')} onClick={() => c().deleteTable().run()}><Trash2 className="text-redpen" /></Tool>
        </>
      )}
    </div>
  )
}

export interface DocEditorHandle {
  setDoc: (doc: PMNode) => void
}

export const DocEditor = React.forwardRef<DocEditorHandle, { doc: unknown; onChange: (doc: PMNode, source: string) => void }>(function DocEditor({ doc, onChange }, ref) {
  const { t } = useTranslation()
  const [math, setMath] = React.useState<{ latex: string; pos: number | null; kind: 'inline' | 'block' } | null>(null)
  const cb = React.useRef(onChange)
  cb.current = onChange
  const editor = useEditor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] }, link: false, code: false, codeBlock: false, strike: false }),
      Superscript,
      Subscript,
      Table.configure({ resizable: false }),
      TableRow,
      TableHeader,
      TableCell,
      PageBreak,
      Placeholder.configure({ placeholder: t('write.placeholder') }),
      Mathematics.configure({
        katexOptions: { throwOnError: false },
        inlineOptions: { onClick: (node, pos) => setMath({ latex: String(node.attrs.latex), pos, kind: 'inline' }) },
        blockOptions: { onClick: (node, pos) => setMath({ latex: String(node.attrs.latex), pos, kind: 'block' }) },
      }),
    ],
    content: (doc as object) ?? '',
    // the system spellchecker has no Uzbek dictionary and would underline every word
    editorProps: { attributes: { 'aria-label': t('write.name'), spellcheck: 'false' } },
    onUpdate: ({ editor: e }) => {
      const json = e.getJSON() as PMNode
      cb.current(json, docToSource(json))
    },
  })

  React.useImperativeHandle(ref, () => ({
    setDoc: (d) => {
      editor?.commands.setContent(d as never)
      const json = editor?.getJSON() as PMNode
      if (json) cb.current(json, docToSource(json))
    },
  }), [editor])

  if (!editor) return null
  return (
    <div className="flex h-full flex-col">
      <Toolbar editor={editor} onMath={() => setMath({ latex: '', pos: null, kind: 'inline' })} />
      <div className="doc-editor min-h-0 flex-1 overflow-auto bg-surface" onClick={(e) => e.target === e.currentTarget && editor.commands.focus('end')}>
        <EditorContent editor={editor} className="h-full" />
      </div>
      <MathDialog open={math !== null} initial={math?.latex ?? ''} onClose={() => setMath(null)}
        onSubmit={(latex) => {
          if (math?.pos != null) {
            if (math.kind === 'inline') editor.chain().setNodeSelection(math.pos).updateInlineMath({ latex }).focus().run()
            else editor.chain().setNodeSelection(math.pos).updateBlockMath({ latex }).focus().run()
          } else {
            editor.chain().focus().insertInlineMath({ latex }).run()
          }
          setMath(null)
        }} />
    </div>
  )
})
