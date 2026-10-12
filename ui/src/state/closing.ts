/** Work to finish before the window closes (a document's last edits). */
const tasks = new Set<() => unknown>()

/** Run `f` before the window closes; returns a function that stops it. */
export function onBeforeClose(f: () => unknown) {
  tasks.add(f)
  return () => {
    tasks.delete(f)
  }
}

/** Finish everything asked for, waiting at most a moment. */
export async function finishBeforeClose() {
  const all = Promise.allSettled([...tasks].map((f) => Promise.resolve().then(f)))
  await Promise.race([all, new Promise((r) => setTimeout(r, 1500))])
}
