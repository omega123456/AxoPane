import { create } from 'zustand'

export type SettingsSection =
  | 'keybindings'
  | 'columns'
  | 'layout'
  | 'searchIndex'
  | 'dates'
  | 'updates'
  | 'logs'

type SettingsStore = {
  isOpen: boolean
  section: SettingsSection
  open: (section?: SettingsSection) => void
  close: () => void
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  isOpen: false,
  section: 'layout',
  open: (section) => set((state) => ({ isOpen: true, section: section ?? state.section })),
  close: () => set({ isOpen: false }),
}))
