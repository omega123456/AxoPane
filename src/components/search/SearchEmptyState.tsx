import type { ReactNode } from 'react'
import { AlertTriangleIcon, SearchIcon } from '@/components/icons'
import { FOCUS_RING } from '@/lib/focus-ring'
import type { SearchRejection } from '@/lib/types/ipc'
import { TEXT_SECONDARY } from '@/lib/class-names'

function stateTileToneClass(warning: boolean) {
  return warning
    ? 'bg-light-warning-bg text-light-warning dark:bg-dark-warning-bg dark:text-dark-warning'
    : `bg-light-hover ${TEXT_SECONDARY} dark:bg-dark-hover`
}

function PaletteState({
  icon,
  warning = false,
  title,
  message,
  action,
}: {
  icon: ReactNode
  warning?: boolean
  title: string
  message: string
  action?: ReactNode
}) {
  return (
    <div role="status" className="grid justify-items-center gap-2 px-6 py-8 text-center">
      <span
        className={`flex size-state-tile items-center justify-center rounded-overlay ${stateTileToneClass(warning)}`}
      >
        {icon}
      </span>
      <h3 className="font-display text-subtitle font-semibold text-light-text dark:text-dark-text">
        {title}
      </h3>
      <p className={`max-w-dialog text-row ${TEXT_SECONDARY}`}>{message}</p>
      {action}
    </div>
  )
}

export function SearchUnavailableState({
  backend,
  rejection,
}: {
  backend: 'everything' | 'macIndex' | 'none'
  rejection: SearchRejection
}) {
  if (rejection.kind === 'failed') {
    return (
      <PaletteState
        warning
        icon={<AlertTriangleIcon className="size-6" />}
        title="Search failed"
        message={rejection.message}
      />
    )
  }
  if (backend === 'everything') {
    return (
      <PaletteState
        warning
        icon={<AlertTriangleIcon className="size-6" />}
        title="Everything isn’t running"
        message="Search uses Everything to find files and folders instantly. Start Everything, or install it, and your search runs as soon as it’s ready."
        action={
          <a
            href="https://www.voidtools.com/downloads/"
            target="_blank"
            rel="noreferrer"
            className={`mt-1 inline-flex h-8 items-center rounded-control bg-light-accent px-3 text-row text-light-on-accent hover:opacity-90 dark:bg-dark-accent dark:text-dark-on-accent ${FOCUS_RING}`}
          >
            Get Everything
          </a>
        }
      />
    )
  }
  return (
    <PaletteState
      icon={<SearchIcon className="size-6" />}
      title={
        rejection.kind === 'notReady' ? 'The folder index is starting' : 'Search is unavailable'
      }
      message={
        rejection.kind === 'notReady'
          ? 'Search works as soon as the folder index has loaded.'
          : 'Folder search needs the folder index, which could not start on this Mac.'
      }
    />
  )
}

export function SearchNoResults({ query, foldersOnly }: { query: string; foldersOnly: boolean }) {
  return (
    <div role="status" className="grid gap-1 px-3.5 py-4">
      <p className="text-body text-light-text dark:text-dark-text">
        {foldersOnly ? `No folders match “${query.trim()}”` : `No results for “${query.trim()}”`}
      </p>
      <p className={`text-caption ${TEXT_SECONDARY}`}>
        {foldersOnly
          ? 'This Mac searches folder names on the startup disk. To find a file, open its folder and type to filter.'
          : 'Check the spelling, drop a path segment, or try a wildcard such as invoice*.'}
      </p>
    </div>
  )
}
