import type { P2 } from './api'

/** 3x3 homography (row-major) mapping 4 src points to 4 dst points. */
export function fitH(src: P2[], dst: P2[]): number[] {
  const a: number[][] = []
  const b: number[] = []
  for (let i = 0; i < 4; i++) {
    const [x, y] = src[i]
    const [u, v] = dst[i]
    a.push([x, y, 1, 0, 0, 0, -u * x, -u * y])
    b.push(u)
    a.push([0, 0, 0, x, y, 1, -v * x, -v * y])
    b.push(v)
  }
  const h = solve(a, b)
  return [...h, 1]
}

function solve(a: number[][], b: number[]): number[] {
  const n = b.length
  const m = a.map((r, i) => [...r, b[i]])
  for (let c = 0; c < n; c++) {
    let p = c
    for (let r = c + 1; r < n; r++) if (Math.abs(m[r][c]) > Math.abs(m[p][c])) p = r
    ;[m[c], m[p]] = [m[p], m[c]]
    const d = m[c][c] || 1e-12
    for (let r = 0; r < n; r++) {
      if (r === c) continue
      const f = m[r][c] / d
      for (let k = c; k <= n; k++) m[r][k] -= f * m[c][k]
    }
  }
  return m.map((r, i) => r[n] / (r[i] || 1e-12))
}

export function applyH(h: number[], [x, y]: P2): P2 {
  const w = h[6] * x + h[7] * y + h[8]
  return [(h[0] * x + h[1] * y + h[2]) / w, (h[3] * x + h[4] * y + h[5]) / w]
}

export function sheetH(corners: P2[], size: [number, number]): number[] {
  const [w, h] = size
  return fitH([[0, 0], [w, 0], [w, h], [0, h]], corners)
}

export function invH(corners: P2[], size: [number, number]): number[] {
  const [w, h] = size
  return fitH(corners, [[0, 0], [w, 0], [w, h], [0, h]])
}
