import type { PaneId } from '@/types/pane'
import { detectPlatformOs, type PlatformOs } from '@/lib/keymap'
import { UNC_PREFIX, WINDOWS_DRIVE_PREFIX_LENGTH } from '@/lib/path-compare'
import { startOp } from '@/lib/queue-commands'
import type { OpKind } from '@/lib/types/ipc'

/** One item being dragged. A minimal projection of `DirectoryEntry`. */
export type DragItem = {
  id: string
  name: string
  path: string
  isDir: boolean
  sizeBytes: number | null
}

/** The active internal drag: which pane/folder it started from and what it carries. */
export type FileTransferDragPayload = {
  kind: 'file-transfer'
  sourcePaneId: PaneId
  /** The directory the dragged items live in (used to reject no-op same-folder drops). */
  sourceDir: string
  items: DragItem[]
}

export type FavouriteDragPayload = {
  kind: 'favourite'
  path: string
}

export type DragPayload = FileTransferDragPayload | FavouriteDragPayload

/** Keyboard modifiers that influence copy-vs-move, sampled at drop time. */
export type DropModifiers = {
  ctrlKey: boolean
  shiftKey: boolean
}

// Windows and default macOS filesystems are case-insensitive, so path
// comparisons are lowercased. Trailing separators are stripped so `C:\a\` and
// `C:\a` compare equal.
function separatorFor(platform: PlatformOs) {
  return platform === 'windows' ? '\\' : '/'
}

function normalizePath(path: string, platform: PlatformOs) {
  const separator = separatorFor(platform)
  // Preserve a leading UNC-style double separator (`\\server\share`); collapse
  // every other run of separators to a single one.
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

/** True when `candidate` is `folder` itself or nested anywhere beneath it. */
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

/**
 * Whether two paths live on the same volume. On Windows that's the drive letter
 * (or UNC share root); on macOS the top-level mount segment (`/`, `/Volumes/X`).
 * Drives the default drop gesture: same volume moves, different volume copies —
 * matching Explorer/Finder.
 */
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
    // UNC path: key on \\server\share; otherwise on the drive letter.
    if (normalized.startsWith(UNC_PREFIX)) {
      const parts = normalized.slice(UNC_PREFIX.length).split('\\')
      return `${UNC_PREFIX}${parts[0] ?? ''}\\${parts[1] ?? ''}`
    }
    return normalized.slice(0, WINDOWS_DRIVE_PREFIX_LENGTH)
  }
  // POSIX: `/Volumes/name` is its own mount; everything else keys on `/`.
  const parts = normalized.split('/').filter(Boolean)
  const mountName = parts[0] === 'volumes' ? parts[1] : undefined
  if (mountName) {
    return `/volumes/${mountName}`
  }
  return '/'
}

/** The two transfer kinds an internal drop can produce. */
export type DropKind = Extract<OpKind, 'copy' | 'move'>

/**
 * Requests the real in-app outcome unless the OS source only permits Copy.
 * Native file drags stay Copy-only so an external target can never delete the
 * source; asking that source for Move makes Windows show a false no-drop cursor.
 */
export function setDropEffect(dataTransfer: DataTransfer, kind: DropKind) {
  dataTransfer.dropEffect = dataTransfer.effectAllowed === 'copy' ? 'copy' : kind
}

type ModifierKeyState = { altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }

const platformDropModifiers: Record<PlatformOs, (event: ModifierKeyState) => DropModifiers> = {
  macos: (event) => ({ ctrlKey: event.altKey, shiftKey: event.metaKey }),
  windows: (event) => ({ ctrlKey: event.ctrlKey, shiftKey: event.shiftKey }),
}

/**
 * Copy-vs-move follows platform convention: on Windows Ctrl forces copy and
 * Shift forces move; on macOS Option forces copy and Cmd forces move. The result
 * is read by `resolveDropKind` as generic force-copy / force-move flags so its
 * logic stays OS-agnostic.
 */
export function dropModifiers(event: ModifierKeyState, platform: PlatformOs): DropModifiers {
  return platformDropModifiers[platform](event)
}

/** Resolve copy vs move: Ctrl forces copy, Shift forces move, else volume-based. */
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

/**
 * Whether `payload` may be dropped into `destinationDir`. Rejects empty drags,
 * trash items (no real path to transfer), dropping into the source folder
 * (no-op), and dropping a folder into itself or its own subtree.
 */
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
  // Same directory: a move would be a no-op and a copy would collide with the
  // originals, so treat dropping onto the source folder as invalid.
  if (normalizedEqual(destinationDir, sourceDir, platform)) {
    return false
  }
  for (const item of items) {
    // Dropping a folder into itself (or its own subtree) is impossible.
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

/**
 * Enqueue the transfer described by an internal drop. Guards are applied first;
 * an invalid drop resolves to `null` without touching the queue. The queue
 * engine + fs watcher refresh the affected panes, so no manual reload is needed.
 */
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
