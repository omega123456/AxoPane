import { create } from 'zustand'

type Section = 'keybindings' | 'columns' | 'layout' | 'dates' | 'updates' | 'logs'

type SettingsStore = {
  isOpen: boolean
  section: Section
  open: (section?: Section) => void
  close: () => void
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  isOpen: false,
  section: 'layout',
  open: (section) =>
    set((state) => ({ isOpen: true, section: section ?? state.section })),
  close: () => set({ isOpen: false }),
}))
