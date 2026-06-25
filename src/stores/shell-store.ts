import { create } from 'zustand'
import type { InitialShellResponse } from '@/lib/types/ipc'

type ShellStore = {
  shell: InitialShellResponse | null
  setShell: (shell: InitialShellResponse) => void
}

export const useShellStore = create<ShellStore>((set) => ({
  shell: null,
  setShell: (shell) => set({ shell }),
}))
