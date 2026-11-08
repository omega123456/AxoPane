import type { StoreApi } from 'zustand'
import type { PanesStore } from '@/stores/panes/store-types'

let boundPanesStore: StoreApi<PanesStore> | null = null

export function bindPanesStore(store: StoreApi<PanesStore>): void {
  boundPanesStore = store
}

export function panesStore(): StoreApi<PanesStore> {
  if (!boundPanesStore) {
    throw new Error('panes store used before panes-store created it')
  }
  return boundPanesStore
}
