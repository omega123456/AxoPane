import type { DirectoryEntry } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'
import {
  createPane,
  defaultPanesData,
  type PaneState,
  type PanesData,
} from '@/stores/panes/pane-state'

export function directoryEntry(
  path: string,
  overrides: Partial<DirectoryEntry> = {},
): DirectoryEntry {
  const name = path.split('/').filter(Boolean).at(-1) ?? path
  return {
    id: path,
    name,
    path,
    isDir: true,
    sizeBytes: null,
    itemCount: null,
    typeLabel: 'Folder',
    modifiedAt: null,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
    ...overrides,
  }
}

export function fileEntry(path: string, overrides: Partial<DirectoryEntry> = {}): DirectoryEntry {
  return directoryEntry(path, { isDir: false, typeLabel: 'File', sizeBytes: 1, ...overrides })
}

export function paneWith(paneId: PaneId, overrides: Partial<PaneState> = {}): PaneState {
  return { ...createPane(paneId, `${paneId} pane`), ...overrides }
}

export function panesData(overrides: Partial<PanesData> = {}): PanesData {
  return { ...defaultPanesData(), ...overrides }
}

export function panesDataWithLeft(left: Partial<PaneState>, overrides: Partial<PanesData> = {}) {
  const base = panesData(overrides)
  return { ...base, panes: { ...base.panes, left: paneWith('left', left) } }
}
