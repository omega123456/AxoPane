import { create } from 'zustand'

export type ContextMenuItem = {
  id: string
  label: string
  shortcut?: string
  disabled?: boolean
  danger?: boolean
  hidden?: boolean
  strong?: boolean
  submenu?: boolean
  separatorBefore?: boolean
  onSelect?: () => void
}

export type ContextMenuState = {
  paneId: 'left' | 'right'
  x: number
  y: number
  title: string
  chip?: string
  items: ContextMenuItem[]
  targetId?: string
}

type Store = {
  menu: ContextMenuState | null
  activeIndex: number
  openMenu: (menu: ContextMenuState) => void
  closeMenu: () => void
  moveActive: (direction: 1 | -1) => void
  activateCurrent: () => void
}

export const useContextMenuStore = create<Store>((set) => ({
  menu: null,
  activeIndex: 0,
  openMenu: (menu) => {
    const visibleItems = menu.items.filter((item) => !item.hidden)
    const activeIndex = visibleItems.findIndex((item) => !item.disabled)
    set({
      menu: {
        ...menu,
        items: visibleItems,
      },
      activeIndex: activeIndex >= 0 ? activeIndex : 0,
    })
  },
  closeMenu: () => set({ menu: null, activeIndex: 0 }),
  moveActive: (direction) =>
    set((state) => {
      if (!state.menu || state.menu.items.length === 0) {
        return state
      }

      const enabledIndexes = state.menu.items
        .map((item, index) => ({ item, index }))
        .filter(({ item }) => !item.disabled)
        .map(({ index }) => index)

      if (enabledIndexes.length === 0) {
        return state
      }

      const currentPosition = Math.max(0, enabledIndexes.indexOf(state.activeIndex))
      const nextPosition =
        (currentPosition + direction + enabledIndexes.length) % enabledIndexes.length

      return { activeIndex: enabledIndexes[nextPosition] }
    }),
  activateCurrent: () =>
    set((state) => {
      const item = state.menu?.items[state.activeIndex]
      if (!item || item.disabled) {
        return state
      }

      item.onSelect?.()
      return { menu: null, activeIndex: 0 }
    }),
}))
