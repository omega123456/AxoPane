import type { DirectoryEntry, IconStateEvent } from '@/lib/types/ipc'
import { withKeyFlags } from '@/lib/collections'
import { utf16ByteWeight } from '@/lib/numbers'
import { pruneIconCache } from '@/stores/panes/cache-policy'
import {
  patchPaneEntries,
  updateEntries,
  type PaneRecord,
  type PaneState,
  type PanesData,
  type ResolvedIconPaths,
} from '@/stores/panes/pane-state'

type IconCache = Pick<PanesData, 'resolvedIconPaths' | 'iconCacheTouched'>

export type IconStatesInput = {
  events: IconStateEvent[]
  now: number
  windowsPlatform: boolean
}

export function hasResolvedIconPath(resolvedIconPaths: ResolvedIconPaths, path: string) {
  return Object.prototype.hasOwnProperty.call(resolvedIconPaths, path)
}

function needsIconRequest(
  entry: DirectoryEntry | undefined,
  state: Pick<PanesData, 'pendingIconRequests' | 'resolvedIconPaths'>,
): entry is DirectoryEntry {
  return (
    entry != null &&
    !entry.isDir &&
    entry.iconDataUrl == null &&
    !hasResolvedIconPath(state.resolvedIconPaths, entry.path) &&
    !state.pendingIconRequests[entry.path]
  )
}

export function collectPendingIconRequests(
  entries: DirectoryEntry[],
  paths: string[],
  state: Pick<PanesData, 'pendingIconRequests' | 'resolvedIconPaths'>,
) {
  const entryByPath = new Map(entries.map((entry) => [entry.path, entry]))
  const unique = Array.from(new Set(paths))
  return unique.filter((path) => needsIconRequest(entryByPath.get(path), state))
}

export function withPendingIconRequests(
  state: Pick<PanesData, 'pendingIconRequests'>,
  paths: string[],
  pending: boolean,
): Pick<PanesData, 'pendingIconRequests'> {
  return { pendingIconRequests: withKeyFlags(state.pendingIconRequests, paths, pending) }
}

export function hydrateEntryIcon(
  entry: DirectoryEntry,
  resolvedIconPaths: ResolvedIconPaths,
): DirectoryEntry {
  if (entry.isDir) {
    return entry
  }
  if (!hasResolvedIconPath(resolvedIconPaths, entry.path)) {
    return entry
  }

  const iconDataUrl = resolvedIconPaths[entry.path]
  if (entry.iconDataUrl === iconDataUrl) {
    return entry
  }
  return { ...entry, iconDataUrl }
}

export function withResolvedIcons(entries: DirectoryEntry[], resolvedIconPaths: ResolvedIconPaths) {
  const nextEntries = entries.map((entry) => hydrateEntryIcon(entry, resolvedIconPaths))
  const changed = nextEntries.some((entry, index) => entry !== entries[index])
  return changed ? nextEntries : entries
}

function entriesWithIcons(pane: PaneState, events: IconStateEvent[]) {
  return updateEntries(
    pane.entries,
    events,
    (event) => pane.entries.findIndex((entry) => entry.path === event.path),
    (entry, event) =>
      entry.iconDataUrl === event.iconDataUrl ? null : { ...entry, iconDataUrl: event.iconDataUrl },
  )
}

function allEntryPaths(panes: PaneRecord) {
  return new Set(Object.values(panes).flatMap((pane) => pane.entries.map((entry) => entry.path)))
}

function boundedIconCache(
  cache: IconCache,
  protectedIconPaths: Set<string>,
  { now, windowsPlatform }: Omit<IconStatesInput, 'events'>,
): IconCache {
  const { resolvedIconPaths, iconCacheTouched } = cache
  const iconRecords = Object.fromEntries(
    Object.entries(resolvedIconPaths).map(([path, value]) => [
      path,
      { value, touched: iconCacheTouched[path] ?? now, weight: utf16ByteWeight(value) },
    ]),
  )
  const retainedIcons = pruneIconCache(iconRecords, protectedIconPaths, now, windowsPlatform)
  return {
    resolvedIconPaths: Object.fromEntries(
      Object.entries(retainedIcons).map(([path, record]) => [path, record.value]),
    ) as ResolvedIconPaths,
    iconCacheTouched: Object.fromEntries(
      Object.keys(retainedIcons).map((path) => [path, iconCacheTouched[path] ?? now]),
    ) as Record<string, number>,
  }
}

function recordedIconResults(cache: IconCache, events: IconStateEvent[], now: number): IconCache {
  const resolvedIconPaths = { ...cache.resolvedIconPaths }
  const iconCacheTouched = { ...cache.iconCacheTouched }
  for (const event of events) {
    resolvedIconPaths[event.path] = event.iconDataUrl
    iconCacheTouched[event.path] = now
  }
  return { resolvedIconPaths, iconCacheTouched }
}

export function appliedIconStates(
  state: Pick<PanesData, 'panes' | 'pendingIconRequests'> & IconCache,
  input: IconStatesInput,
): Partial<PanesData> {
  const { events, now } = input
  if (events.length === 0) {
    return {}
  }

  const panes = patchPaneEntries(state.panes, (_paneId, pane) => entriesWithIcons(pane, events))
  const recorded = recordedIconResults(state, events, now)
  const paths = events.map((event) => event.path)
  return {
    panes: panes ?? state.panes,
    ...withPendingIconRequests(state, paths, false),
    ...boundedIconCache(recorded, allEntryPaths(state.panes), input),
  }
}
