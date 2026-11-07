import { createDialogStore } from '@/stores/dialog-store-factory'
import type { MacApp } from '@/lib/types/ipc'

export type DefaultAppDialogState = {
  filePath: string
  fileName: string
  apps: MacApp[]
}

export const useDefaultAppDialogStore = createDialogStore<DefaultAppDialogState>()
