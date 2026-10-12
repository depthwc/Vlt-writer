import { create } from 'zustand'

import { onJob, type JobEvent } from '@/lib/api'

interface JobsState {
  jobs: Record<number, JobEvent>
}

export const useJobs = create<JobsState>(() => ({ jobs: {} }))

type Waiter = { resolve: (v: unknown) => void; reject: (e: Error) => void; progress?: (e: JobEvent) => void }
const waiters = new Map<number, Waiter>()
const finished = new Map<number, JobEvent>()

function settle(e: JobEvent) {
  const w = waiters.get(e.id)
  if (!w) {
    finished.set(e.id, e)
    return
  }
  waiters.delete(e.id)
  if (e.error) w.reject(new Error(e.error))
  else w.resolve(e.result)
}

void onJob((e) => {
  useJobs.setState((s) => {
    const jobs = { ...s.jobs }
    if (e.finished) delete jobs[e.id]
    else jobs[e.id] = e
    return { jobs }
  })
  waiters.get(e.id)?.progress?.(e)
  if (e.finished) settle(e)
})

/** Start a backend job and wait for its result. */
export async function runJob<T>(start: () => Promise<number>, progress?: (e: JobEvent) => void): Promise<T> {
  const id = await start()
  return new Promise<T>((resolve, reject) => {
    waiters.set(id, { resolve: resolve as (v: unknown) => void, reject, progress })
    const done = finished.get(id)
    if (done) {
      finished.delete(id)
      settle(done)
    }
  })
}
