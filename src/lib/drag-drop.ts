import type { PaneId } from '@/types/pane'
import { detectPlatformOs, type PlatformOs } from '@/lib/keymap'
import {
  UNC_PREFIX,
  WINDOWS_DRIVE_PREFIX_LENGTH,
  POSIX_SEPARATOR,
  WINDOWS_SEPARATOR,
} from '@/lib/path-compare'
import { startOp } from '@/lib/queue-commands'
import type { OpKind } from '@/lib/types/ipc'

export type DragItem = {
  id: string
  name: string
  path: string
  isDir: boolean
  sizeBytes: number | null
}

export type FileTransferDragPayload = {
  kind: 'file-transfer'
  sourcePaneId: PaneId
  sourceDir: string
  items: DragItem[]
}

export type FavouriteDragPayload = {
  kind: 'favourite'
  path: string
}

export type DragPayload = FileTransferDragPayload | FavouriteDragPayload

export type DropModifiers = {
  ctrlKey: boolean
  shiftKey: boolean
}

function separatorFor(platform: PlatformOs) {
  return platform === 'windows' ? WINDOWS_SEPARATOR : POSIX_SEPARATOR
}

function normalizePath(path: string, platform: PlatformOs) {
  const separator = separatorFor(platform)
  const hasLeadingDouble = /^[\\/]{2}/.test(path)
  const leading = hasLeadingDouble ? separator + separator : ''
  let normalized = leading + path.slice(leading.length).replace(/[\\/]+/g, separator)
  const minimumLength = leading.length + 1
  const hasTrimmableSeparator = (candidate: string) =>
    candidate.length > minimumLength && candidate.endsWith(separator)
  while (hasTrimmableSeparator(normalized)) {
    normalized = normalized.slice(0, -1)
  }
  return normalized.toLowerCase()
}

export function isSameOrDescendant(
  candidate: string,
  folder: string,
  platform: PlatformOs = detectPlatformOs(),
) {
  const separator = separatorFor(platform)
  const normalizedCandidate = normalizePath(candidate, platform)
  const normalizedFolder = normalizePath(folder, platform)
  return (
    normalizedCandidate === normalizedFolder ||
    normalizedCandidate.startsWith(normalizedFolder + separator)
  )
}

export function sameVolume(
  leftPath: string,
  rightPath: string,
  platform: PlatformOs = detectPlatformOs(),
) {
  return volumeKey(leftPath, platform) === volumeKey(rightPath, platform)
}

function volumeKey(path: string, platform: PlatformOs) {
  const normalized = normalizePath(path, platform)
  if (platform === 'windows') {
    if (normalized.startsWith(UNC_PREFIX)) {
      const parts = normalized.slice(UNC_PREFIX.length).split(WINDOWS_SEPARATOR)
      return `${UNC_PREFIX}${parts[0] ?? ''}${WINDOWS_SEPARATOR}${parts[1] ?? ''}`
    }
    return normalized.slice(0, WINDOWS_DRIVE_PREFIX_LENGTH)
  }
  const parts = normalized.split('/').filter(Boolean)
  const mountName = parts[0] === 'volumes' ? parts[1] : undefined
  if (mountName) {
    return `/volumes/${mountName}`
  }
  return '/'
}

export type DropKind = Extract<OpKind, 'copy' | 'move'>

export function setDropEffect(dataTransfer: DataTransfer, kind: DropKind) {
  dataTransfer.dropEffect = dataTransfer.effectAllowed === 'copy' ? 'copy' : kind
}

type ModifierKeyState = { altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }

const platformDropModifiers: Record<PlatformOs, (event: ModifierKeyState) => DropModifiers> = {
  macos: (event) => ({ ctrlKey: event.altKey, shiftKey: event.metaKey }),
  windows: (event) => ({ ctrlKey: event.ctrlKey, shiftKey: event.shiftKey }),
}

export function dropModifiers(event: ModifierKeyState, platform: PlatformOs): DropModifiers {
  return platformDropModifiers[platform](event)
}

export type DropRoute = {
  sourceDir: string
  destinationDir: string
}

export function resolveDropKind(
  modifiers: DropModifiers,
  { sourceDir, destinationDir }: DropRoute,
  platform: PlatformOs = detectPlatformOs(),
): DropKind {
  if (modifiers.ctrlKey) {
    return 'copy'
  }
  if (modifiers.shiftKey) {
    return 'move'
  }
  return sameVolume(sourceDir, destinationDir, platform) ? 'move' : 'copy'
}

export function canDropInto(
  payload: DragPayload | null,
  destinationDir: string,
  platform: PlatformOs = detectPlatformOs(),
): boolean {
  if (payload?.kind !== 'file-transfer') {
    return false
  }
  const { items, sourceDir } = payload
  if (items.length === 0) {
    return false
  }
  if (normalizedEqual(destinationDir, sourceDir, platform)) {
    return false
  }
  for (const item of items) {
    const intoOwnSubtree = item.isDir && isSameOrDescendant(destinationDir, item.path, platform)
    if (intoOwnSubtree) {
      return false
    }
  }
  return true
}

function normalizedEqual(leftPath: string, rightPath: string, platform: PlatformOs) {
  return normalizePath(leftPath, platform) === normalizePath(rightPath, platform)
}

export type DropRequest = {
  destinationDir: string
  modifiers: DropModifiers
}

export async function performDrop(
  payload: DragPayload | null,
  { destinationDir, modifiers }: DropRequest,
  platform: PlatformOs = detectPlatformOs(),
): Promise<string | null> {
  if (!canDropInto(payload, destinationDir, platform)) {
    return null
  }
  if (payload?.kind !== 'file-transfer') return null
  const route = { sourceDir: payload.sourceDir, destinationDir }
  const kind = resolveDropKind(modifiers, route, platform)
  return startOp({
    kind,
    destinationDir,
    items: payload.items.map((item) => ({
      sourcePath: item.path,
      name: item.name,
      sizeBytes: item.sizeBytes ?? 0,
    })),
  })
}
