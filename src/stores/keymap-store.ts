import { create } from 'zustand'
import type { CommandId, Shortcut } from '@/lib/types/ipc'
import { defaultKeymap, mergeKeymap } from '@/lib/keymap'

type KeymapState = {
  bindings: Record<CommandId, Shortcut[]>
  hydrate: (bindings: Partial<Record<CommandId, Shortcut[]>>) => void
  setBinding: (commandId: CommandId, shortcuts: Shortcut[]) => void
  resetBinding: (commandId: CommandId) => void
  reset: () => void
}

export const useKeymapStore = create<KeymapState>((set) => ({
  bindings: mergeKeymap({}),
  hydrate: (bindings) => set({ bindings: mergeKeymap(bindings) }),
  setBinding: (commandId, shortcuts) =>
    set((state) => ({
      bindings: {
        ...state.bindings,
        [commandId]: shortcuts,
      },
    })),
  resetBinding: (commandId) =>
    set((state) => ({
      bindings: {
        ...state.bindings,
        [commandId]: defaultKeymap[commandId],
      },
    })),
  reset: () => set({ bindings: mergeKeymap({}) }),
}))
