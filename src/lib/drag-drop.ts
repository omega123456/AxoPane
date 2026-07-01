import type { PaneId } from '@/types/pane'
import { detectPlatformOs, type PlatformOs } from '@/lib/keymap'
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
export type DragPayload = {
  sourcePaneId: PaneId
  /** The directory the dragged items live in (used to reject no-op same-folder drops). */
  sourceDir: string
  items: DragItem[]
}

/** Keyboard modifiers that influence copy-vs-move, sampled at drop time. */
export type DropModifiers = {
  ctrlKey: boolean
  shiftKey: boolean
}

// Windows and default macOS filesystems are case-insensitive, so path
// comparisons are lowercased. Trailing separators are stripped so `C:\a\` and
// `C:\a` compare equal.
function separator(os: PlatformOs) {
  return os === 'windows' ? '\\' : '/'
}

function normalizePath(path: string, os: PlatformOs) {
  const sep = separator(os)
  // Preserve a leading UNC-style double separator (`\\server\share`); collapse
  // every other run of separators to a single one.
  const hasLeadingDouble = /^[\\/]{2}/.test(path)
  const leading = hasLeadingDouble ? sep + sep : ''
  let normalized = leading + path.slice(hasLeadingDouble ? 2 : 0).replace(/[\\/]+/g, sep)
  while (normalized.length > leading.length + 1 && normalized.endsWith(sep)) {
    normalized = normalized.slice(0, -1)
  }
  return normalized.toLowerCase()
}

/** True when `candidate` is `folder` itself or nested anywhere beneath it. */
export function isSameOrDescendant(candidate: string, folder: string, os: PlatformOs = detectPlatformOs()) {
  const sep = separator(os)
  const a = normalizePath(candidate, os)
  const b = normalizePath(folder, os)
  return a === b || a.startsWith(b + sep)
}

/**
 * Whether two paths live on the same volume. On Windows that's the drive letter
 * (or UNC share root); on macOS the top-level mount segment (`/`, `/Volumes/X`).
 * Drives the default drop gesture: same volume moves, different volume copies —
 * matching Explorer/Finder.
 */
export function sameVolume(a: string, b: string, os: PlatformOs = detectPlatformOs()) {
  return volumeKey(a, os) === volumeKey(b, os)
}

function volumeKey(path: string, os: PlatformOs) {
  const normalized = normalizePath(path, os)
  if (os === 'windows') {
    // UNC path: key on \\server\share; otherwise on the drive letter.
    if (normalized.startsWith('\\\\')) {
      const parts = normalized.slice(2).split('\\')
      return `\\\\${parts[0] ?? ''}\\${parts[1] ?? ''}`
    }
    return normalized.slice(0, 2)
  }
  // POSIX: `/Volumes/name` is its own mount; everything else keys on `/`.
  const parts = normalized.split('/').filter(Boolean)
  if (parts[0] === 'volumes' && parts[1]) {
    return `/volumes/${parts[1]}`
  }
  return '/'
}

/** The two transfer kinds an internal drop can produce. */
export type DropKind = Extract<OpKind, 'copy' | 'move'>

/** Resolve copy vs move: Ctrl forces copy, Shift forces move, else volume-based. */
export function resolveDropKind(
  modifiers: DropModifiers,
  sourceDir: string,
  destinationDir: string,
  os: PlatformOs = detectPlatformOs(),
): DropKind {
  if (modifiers.ctrlKey) {
    return 'copy'
  }
  if (modifiers.shiftKey) {
    return 'move'
  }
  return sameVolume(sourceDir, destinationDir, os) ? 'move' : 'copy'
}

/**
 * Whether `payload` may be dropped into `destinationDir`. Rejects empty drags,
 * trash items (no real path to transfer), dropping into the source folder
 * (no-op), and dropping a folder into itself or its own subtree.
 */
export function canDropInto(
  payload: DragPayload | null,
  destinationDir: string,
  os: PlatformOs = detectPlatformOs(),
): boolean {
  if (!payload || payload.items.length === 0) {
    return false
  }
  // Same directory: a move would be a no-op and a copy would collide with the
  // originals, so treat dropping onto the source folder as invalid.
  if (normalizedEqual(destinationDir, payload.sourceDir, os)) {
    return false
  }
  for (const item of payload.items) {
    // Dropping a folder into itself (or its own subtree) is impossible.
    if (item.isDir && isSameOrDescendant(destinationDir, item.path, os)) {
      return false
    }
  }
  return true
}

function normalizedEqual(a: string, b: string, os: PlatformOs) {
  return normalizePath(a, os) === normalizePath(b, os)
}

/**
 * Enqueue the transfer described by an internal drop. Guards are applied first;
 * an invalid drop resolves to `null` without touching the queue. The queue
 * engine + fs watcher refresh the affected panes, so no manual reload is needed.
 */
export async function performDrop(
  payload: DragPayload | null,
  destinationDir: string,
  modifiers: DropModifiers,
  os: PlatformOs = detectPlatformOs(),
): Promise<string | null> {
  if (!payload || !canDropInto(payload, destinationDir, os)) {
    return null
  }
  const kind = resolveDropKind(modifiers, payload.sourceDir, destinationDir, os)
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
