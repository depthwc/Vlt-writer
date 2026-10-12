import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ---------------------------------------------------------------- types (mirror hw-core / src-tauri)
export type Src = 'box' | 'text' | 'auto'
export type How = 'case' | 'mark' | 'join' | 'draw' | 'font'
export type Flag = 'cut' | 'faint' | 'size' | 'duplicate'
export type Lang = 'uz_latn' | 'uz_cyrl' | 'ru' | 'en' | 'math'
export type Level = 'quick' | 'normal' | 'best'
export type PageKind = 'latin' | 'cyrillic' | 'digits' | 'math' | 'words' | 'refill'
export type PenKind = 'mine' | 'ballpoint' | 'gel' | 'pencil' | 'fountain' | 'felt'
export type PaperKind = 'lined' | 'grid' | 'blank'
/** a notebook's left or right page, or any paper (a loose sheet) */
export type PaperSide = 'left' | 'right' | 'any'
/** project.paper: a paper id, or a notebook (left and right pages in turn) */
export const NOTEBOOK = 'notebook'
/** Every page on a photo picked at random from all papers. */
export const RANDOM = 'random'
export type P2 = [number, number]

export type Edit =
  | { op: 'erase'; pts: P2[]; r: number }
  | { op: 'restore'; pts: P2[]; r: number }
  | { op: 'rotate'; deg: number }
  | { op: 'slant'; k: number }
  | { op: 'scale'; f: number }

export interface Summary { id: string; name: string; created: number; count: number }

export interface GlyphView { id: string; text: string; src: Src; w: number; h: number; bl: number; on: boolean; flags: Flag[]; edits: number; how: How | null; from: string; print: boolean }
export interface HandMeta {
  id: string; name: string; created: number; ink: [number, number, number]; ink_n: number; xh_mm: number; box_scale: number
  stats: { gap: [number, number]; word: [number, number]; join: number; pairs: Record<string, [number, number]> }
  imports: unknown[]; langs: Lang[]; lean: number; cap: number
}
/** n = written versions */
export interface CovChar { c: string; n: number }
export interface HandView { meta: HandMeta; glyphs: GlyphView[]; coverage: { group: string; chars: CovChar[] }[]; refill: number }

export interface SheetItem { kind: 'glyph' | 'line'; text: string; r: [number, number, number, number]; bl: number }
export interface SheetPage { index: number; code: [number, number]; kind: PageKind; lang: Lang | null; items: SheetItem[] }
export interface Recipe { langs: Lang[]; level: Level }
/** A page to write by hand; done = its photo was read */
export interface GuidePage { id: string; lang: Lang | null; kind: 'joined' | 'print' | 'separate' | 'math' | 'extra' | 'fill'; n: number; lines: string[]; print: boolean[]; done: { letters: number; unmatched: string[]; small: boolean } | null }
/** required: every built-in page by language; missing: pages of what did not come through;
 *  own: the person's own writing, photos with their text */
export interface GuideView { main: Lang; required: { lang: Lang; pages: GuidePage[] }[]; missing: GuidePage[]; fill: GuidePage[]; own: { import: number; letters: number; name: string; small: boolean }[] }
/** a folder's photos paired with the .txt of the same name */
export interface FolderScan { pages: { name: string; images: string[]; text: string; lines: number; again: boolean }[]; no_text: string[]; no_photo: string[] }
/** Words printed on the sheet, in the interface language. */
export interface SheetText { head: string; boxes: string; words: string; refill: string; titles: Record<string, string> }

export interface Shot { file: string; w: number; h: number; corners: [P2, P2, P2, P2]; found: boolean; rules: P2[][]; rules_edited: boolean; vlines: number[]; margin_mm: number | null; noise: number; blur_px: number; ppm_photo: number; area_bend?: [number[], number[]] | null }
export interface PaperMeta { id: string; name: string; created: number; size: string; kind: PaperKind; step: number; pitch_rule: number; start_line: number; left_mm: number; right_mm: number; shots: Shot[]; side: PaperSide }
export interface PaperView { meta: PaperMeta; lines_px: P2[][][] }

/** which letters to write with: print for English and joined for the rest, or one of them */
export type Letters = 'auto' | 'joined' | 'print'
export interface Params { size: number; mess: number; joins: number; indent_mm: number; seed: number; slant: number; letter_spacing: number; word_spacing: number; line_drift: number; speed: number; mistakes: number; overrides: Record<string, string>; letters: Letters }
export interface Pen { kind: PenKind; color: [number, number, number] | null; density: number; ink_low: number }
export interface ShotStyle { strength: number; angle: boolean; angle_max?: number; light: boolean; shadow: boolean; noise: boolean }
export interface Placement { start_line?: number | null; left_mm?: number | null; right_mm?: number | null; mesh?: P2[][] | null; shot?: number | null }
export interface Project {
  id: string; name: string; created: number; updated: number; hand: string | null; doc: unknown; blocks: unknown[]
  source?: string; params: Params; pen: Pen; shot: ShotStyle; paper: string | null; place: Record<string, Placement>; outputs: string[]
}
/** missing: characters never written, with how often the document uses each */
export interface PageSheet { paper: string; name: string; side: PaperSide; shot: number; w: number; h: number }
export interface LayoutInfo { sheets: PageSheet[]; pages: number; clean_pages: number; missing: [string, number][]; missing_print: [string, number][]; sheet_w_mm: number; lines: number; lines_per_page: number; start_line: number; left_mm: number; right_mm: number; nodes: P2[][]; mesh: P2[][]; w: number; h: number; shot: number; shots: number; paper: string | null; side: PaperSide }
export interface JobEvent { id: number; label: string; done: number; total: number; message: string; finished: boolean; error: string | null; result: unknown }
export interface Report { ok: boolean; error: string | null; page: number | null; boxes: number; lines: number; glyphs: number; warnings: string[]; px_mm: number }
/** below this many px per mm a photo gives blurry letters */
export const SHARP_PX_MM = 8
export type Block = { t: 'h'; text: string; level: number } | { t: 'p'; text: string } | { t: 'li'; text: string; marker: string } | { t: 'table'; rows: string[][] } | { t: 'math'; tex: string } | { t: 'rule' } | { t: 'br' }

export interface GcodeProfile { pen_up: string; pen_down: string; feed: number; travel: number; dwell: number; flip_y: boolean; page_h: number; offset: [number, number] }
export interface ExportReq {
  project: string; paper: string | null; formats: string[]; pages: number[] | null; shots_per_page: number; random: boolean
  quality: number; width: number; folder: string; template: string; style: ShotStyle | null; gcode: GcodeProfile | null
}

// ---------------------------------------------------------------- environment
export const inTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** URL of an image served by the app (custom protocol). */
export function hw(path: string, q: Record<string, string | number | undefined> = {}) {
  const qs = Object.entries(q)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `${k}=${encodeURIComponent(String(v))}`)
    .join('&')
  return `http://hw.localhost/${path}${qs ? `?${qs}` : ''}`
}

type Mock = (cmd: string, args: Record<string, unknown>) => unknown
let mock: Mock | null = null
export function setMock(m: Mock) {
  mock = m
}

async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (mock) return mock(cmd, args) as T
  return invoke<T>(cmd, args)
}

export function onJob(fn: (e: JobEvent) => void): Promise<UnlistenFn> {
  if (!inTauri()) return Promise.resolve(() => {})
  return listen<JobEvent>('job', (e) => fn(e.payload))
}

// ---------------------------------------------------------------- commands
export const api = {
  listHands: () => call<Summary[]>('list_hands'),
  createHand: (name: string) => call<string>('create_hand', { name }),
  renameHand: (id: string, name: string) => call<void>('rename_hand', { id, name }),
  deleteHand: (id: string) => call<void>('delete_hand', { id }),
  getHand: (id: string) => call<HandView>('get_hand', { id }),
  sheetPlan: (recipe: Recipe | null, hand: string | null) => call<SheetPage[]>('sheet_plan', { recipe, hand }),
  saveSheetPdf: (path: string, recipe: Recipe | null, hand: string | null, text: SheetText) => call<number>('save_sheet_pdf', { path, recipe, hand, text }),
  setHandLangs: (hand: string, langs: Lang[]) => call<HandView>('set_hand_langs', { hand, langs }),
  importSheetPhotos: (hand: string, paths: string[]) => call<number>('import_sheet_photos', { hand, paths }),
  importHandwritingPhoto: (hand: string, paths: string[], transcript: string, size: string, print = false) => call<number>('import_handwriting_photo', { hand, paths, transcript, size, print }),
  photoPreview: (path: string) => call<string>('photo_preview', { path }),
  guidePages: (hand: string) => call<GuideView>('guide_pages', { hand }),
  importGuidePage: (hand: string, page: string, lines: string[], print: boolean[], paths: string[]) => call<number>('import_guide_page', { hand, page, lines, print, paths }),
  removeImport: (hand: string, importId: number) => call<HandView>('remove_import', { hand, import: importId }),
  readTextFile: (path: string) => call<string>('read_text_file', { path }),
  photoText: (path: string) => call<string | null>('photo_text', { path }),
  scanFolder: (hand: string, dir: string) => call<FolderScan>('scan_folder', { hand, dir }),
  importOwnPages: (hand: string, pages: { images: string[]; text: string }[]) => call<number>('import_own_pages', { hand, pages }),
  updateGlyph: (hand: string, id: string, patch: { text?: string; on?: boolean; bl?: number; edits?: Edit[] }) => call<void>('update_glyph', { hand, id, patch }),
  glyphEdits: (hand: string, id: string) => call<Edit[]>('glyph_edits', { hand, id }),
  splitGlyph: (hand: string, id: string, x: number, left: string, right: string) => call<void>('split_glyph', { hand, id, x, left, right }),
  deleteGlyphs: (hand: string, ids: string[]) => call<void>('delete_glyphs', { hand, ids }),
  setGlyphsPrint: (hand: string, ids: string[], print: boolean) => call<void>('set_glyphs_print', { hand, ids, print }),
  setHandStats: (hand: string, join?: number, xhMm?: number) => call<void>('set_hand_stats', { hand, join, xhMm }),
  exportHand: (id: string, path: string) => call<void>('export_hand', { id, path }),
  importHand: (path: string) => call<string>('import_hand', { path }),

  listPapers: () => call<PaperMeta[]>('list_papers'),
  getPaper: (id: string) => call<PaperView>('get_paper', { id }),
  addPaper: (name: string, path: string, side: PaperSide) => call<PaperView>('add_paper', { name, path, side }),
  addPaperShot: (id: string, path: string) => call<PaperView>('add_paper_shot', { id, path }),
  calibratePaper: (id: string, c: { shot: number; corners?: P2[] | null; size?: string | null; step?: number | null }) => call<PaperView>('calibrate_paper', { id, c }),
  updatePaper: (id: string, patch: { name?: string; start_line?: number; left_mm?: number; right_mm?: number; kind?: PaperKind; side?: PaperSide; rules?: [number, P2[][]]; area_bend?: [number, [number[], number[]]]; remove_shot?: number }) => call<PaperView>('update_paper', { id, patch }),
  deletePaper: (id: string) => call<void>('delete_paper', { id }),

  listProjects: () => call<Summary[]>('list_projects'),
  createProject: (name: string, hand: string | null) => call<Project>('create_project', { name, hand }),
  getProject: (id: string) => call<Project>('get_project', { id }),
  saveProject: (project: Project) => call<Project>('save_project', { project }),
  deleteProject: (id: string) => call<void>('delete_project', { id }),
  importDocument: (path: string) => call<Block[]>('import_document', { path }),
  /** Show a folder in the file manager. */
  openFolder: (path: string) => call<void>('open_folder', { path }),
  htmlToBlocks: (html: string) => call<Block[]>('html_to_blocks', { html }),
  layoutInfo: (project: string, paper: string | null, page = 0) => call<LayoutInfo>('layout_info', { project, paper, page }),
  cycleGlyph: (project: string, w: number, c: number) => call<string | null>('cycle_glyph', { project, w, c }),
  exportProject: (req: ExportReq) => call<number>('export_project', { req }),
  cancelJob: (id: number) => call<void>('cancel_job', { id }),

  getSetting: (key: string) => call<string | null>('get_setting', { key }),
  setSetting: (key: string, value: string) => call<void>('set_setting', { key, value }),
  allSettings: () => call<[string, string][]>('all_settings'),
  testCopy: () => call<boolean>('test_copy'),
  libraryPath: () => call<string>('library_path'),
  backupLibrary: (path: string) => call<void>('backup_library', { path }),
  paperSizes: () => call<[string, number, number][]>('paper_sizes'),
  penPresets: () => call<[string, [number, number, number]][]>('pen_presets'),
}
