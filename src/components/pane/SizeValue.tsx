import type { ReactNode } from 'react'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { LoaderCircleIcon } from '@/components/icons'
import { usePanesStore } from '@/stores/panes-store'
import { EMPTY_VALUE, formatBytes } from '@/lib/format'

type SizeValueProps = {
  entry: DirectoryEntry
}

const DENIED_TITLE = 'macOS blocked access. Grant Full Disk Access to include this folder.'

type DirectorySizeState = ReturnType<typeof usePanesStore.getState>['sizeStates'][string]

const NO_SIZE = <span>—</span>

function CalculatingSize() {
  return (
    <span className="inline-flex items-center justify-end gap-1 text-light-accent dark:text-dark-accent">
      <LoaderCircleIcon className="h-3.5 w-3.5 animate-spin" />
      <span className="sr-only">Calculating size</span>
    </span>
  )
}

function readySize(sizeState: DirectorySizeState) {
  if (sizeState.sizeBytes === null) {
    return NO_SIZE
  }
  if (!sizeState.isLowerBound) {
    return <span>{formatBytes(sizeState.sizeBytes)}</span>
  }
  return <span title={lowerBoundTitle(sizeState)}>≥ {formatBytes(sizeState.sizeBytes)}</span>
}

function unavailableSize(sizeState: DirectorySizeState) {
  return <span title={sizeState.naReason === 'denied' ? DENIED_TITLE : undefined}>N/A</span>
}

const directorySizeRenderers: Record<
  DirectorySizeState['state'],
  (sizeState: DirectorySizeState) => ReactNode
> = {
  unknown: () => NO_SIZE,
  calculating: () => <CalculatingSize />,
  ready: readySize,
  na: unavailableSize,
  error: unavailableSize,
}

function DirectorySize({ sizeState }: { sizeState: DirectorySizeState | undefined }) {
  return sizeState ? directorySizeRenderers[sizeState.state](sizeState) : NO_SIZE
}

export function SizeValue({ entry }: SizeValueProps) {
  const sizeState = usePanesStore((state) => state.sizeStates[entry.path])

  if (entry.isDir) {
    return <DirectorySize sizeState={sizeState} />
  }

  return <span>{entry.sizeBytes === null ? EMPTY_VALUE : formatBytes(entry.sizeBytes)}</span>
}

const SINGLE_ITEM = 1
const NO_MORE = 0
const LIST_SEPARATOR = ', '
const SENTENCE_SEPARATOR = ' '

function withMore(names: string[], more: number) {
  return more > NO_MORE ? [...names, `${more} more`] : names
}

function joinedList(items: string[]) {
  if (items.length === SINGLE_ITEM) {
    return items[0]
  }
  return `${items.slice(0, -1).join(LIST_SEPARATOR)} and ${items[items.length - 1]}`
}

function privacySkippedSentence(names: string[], more: number) {
  if (names.length === 0) {
    return "Doesn't include folders macOS blocked access to."
  }
  const items = withMore(names, more)
  const list = joinedList(items)
  return items.length === SINGLE_ITEM
    ? `Doesn't include ${list}. Open it to include it.`
    : `Doesn't include ${list}. Open them to include them.`
}

function userExcludedSentence(names: string[], more: number) {
  const items = withMore(names, more)
  const list = joinedList(items)
  return items.length === SINGLE_ITEM
    ? `Doesn't include ${list}, which is excluded in Search index settings.`
    : `Doesn't include ${list}, which are excluded in Search index settings.`
}

function lowerBoundTitle(sizeState: DirectorySizeState) {
  const privacySkipped = sizeState.skippedRoots ?? []
  const userExcluded = sizeState.excludedRoots ?? []
  const more = sizeState.skippedMore ?? NO_MORE
  if (userExcluded.length === 0) {
    return privacySkippedSentence(privacySkipped, more)
  }
  if (privacySkipped.length === 0) {
    return userExcludedSentence(userExcluded, more)
  }
  return [
    privacySkippedSentence(privacySkipped, more),
    userExcludedSentence(userExcluded, NO_MORE),
  ].join(SENTENCE_SEPARATOR)
}
