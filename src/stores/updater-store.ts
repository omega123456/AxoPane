import { create } from 'zustand'
import type { AppUpdate, UpdateSummary } from '@/lib/updater'

type UpdaterStatus = 'idle' | 'installing' | 'error'

type UpdaterStore = {
  update: AppUpdate | null
  summary: UpdateSummary | null
  status: UpdaterStatus
  error: string | null
  setAvailable: (update: AppUpdate, summary: UpdateSummary) => void
  setStatus: (status: UpdaterStatus, error?: string | null) => void
  dismiss: () => void
}

export const useUpdaterStore = create<UpdaterStore>((set) => ({
  update: null,
  summary: null,
  status: 'idle',
  error: null,
  setAvailable: (update, summary) => set({ update, summary, status: 'idle', error: null }),
  setStatus: (status, error = null) => set({ status, error }),
  dismiss: () => set({ update: null, summary: null, status: 'idle', error: null }),
}))
