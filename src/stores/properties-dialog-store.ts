import { createDialogStore } from '@/stores/dialog-store-factory'
import type { DirectoryEntry } from '@/lib/types/ipc'

export type PropertiesDialogItem = Pick<
  DirectoryEntry,
  | 'attributes'
  | 'createdAt'
  | 'id'
  | 'isDir'
  | 'isHidden'
  | 'isSystem'
  | 'itemCount'
  | 'modifiedAt'
  | 'name'
  | 'path'
  | 'sizeBytes'
  | 'typeLabel'
>

export type PropertiesDialogState = {
  items: PropertiesDialogItem[]
}

export const usePropertiesDialogStore = createDialogStore<PropertiesDialogState>()
