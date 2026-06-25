import { create } from 'zustand'

type Section = 'keybindings' | 'columns' | 'layout'

type SettingsStore = {
  isOpen: boolean
  section: Section
  open: (section?: Section) => void
  close: () => void
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  isOpen: false,
  section: 'keybindings',
  open: (section = 'keybindings') => set({ isOpen: true, section }),
  close: () => set({ isOpen: false }),
}))
