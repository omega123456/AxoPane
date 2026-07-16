import { useEffect, useRef, useState } from 'react'
import type { PaneState } from '@/types/pane'
import { ChevronRightIcon } from '@/components/icons'
import type { BreadcrumbLayoutMeasure } from '@/lib/breadcrumb-layout'
import { computeBreadcrumbLayout, createBreadcrumbMeasurer } from '@/lib/breadcrumb-layout'
import { useElementWidth } from '@/lib/use-element-width'
import { usePanesStore } from '@/stores/panes-store'

type BreadcrumbBarProps = {
  pane: PaneState
}

export function BreadcrumbBar({ pane }: BreadcrumbBarProps) {
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const [editingPath, setEditingPath] = useState(false)
  const [pathDraft, setPathDraft] = useState(pane.path)
  const pathInputRef = useRef<HTMLInputElement>(null)
  const [navElement, setNavElement] = useState<HTMLElement | null>(null)
  const [measureVersion, setMeasureVersion] = useState(0)

  const segments = splitPath(pane.path)
  const navWidth = useElementWidth(navElement)
  const layout = computeBreadcrumbLayout({
    segments,
    availableWidth: navWidth,
    measure:
      navWidth > 0
        ? createBreadcrumbMeasurer(navElement)
        : ({
            segment: () => Number.NaN,
          } satisfies BreadcrumbLayoutMeasure),
  })

  useEffect(() => {
    if (!editingPath) {
      return
    }

    pathInputRef.current?.focus()
    pathInputRef.current?.select()
  }, [editingPath])

  useEffect(() => {
    let active = true
    const bumpMeasureVersion = () => {
      if (!active) {
        return
      }

      setMeasureVersion((version) => version + 1)
    }

    const animationFrame = window.requestAnimationFrame(bumpMeasureVersion)
    const fonts = document.fonts
    if (!fonts) {
      return () => {
        active = false
        window.cancelAnimationFrame(animationFrame)
      }
    }

    void fonts.ready.then(bumpMeasureVersion)
    fonts.addEventListener?.('loadingdone', bumpMeasureVersion)

    return () => {
      active = false
      window.cancelAnimationFrame(animationFrame)
      fonts.removeEventListener?.('loadingdone', bumpMeasureVersion)
    }
  }, [])

  void measureVersion

  function startPathEdit() {
    setPathDraft(pane.path)
    setEditingPath(true)
  }

  function submitPathEdit() {
    const trimmed = pathDraft.trim()
    setEditingPath(false)
    if (trimmed && trimmed !== pane.path) {
      void navigatePane(pane.id, trimmed)
    }
  }

  function navigateShortcut(path: string) {
    setEditingPath(false)
    void navigatePane(pane.id, path)
  }

  return (
    <div
      className="flex h-crumb items-center gap-2 border-b border-light-border bg-light-surface px-3 dark:border-dark-border dark:bg-dark-surface"
      onBlur={(event) => {
        if (
          editingPath &&
          !(
            event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget)
          )
        ) {
          submitPathEdit()
        }
      }}
      onContextMenu={(event) => {
        event.preventDefault()
        event.stopPropagation()
        startPathEdit()
      }}
    >
      {editingPath ? (
        <input
          ref={pathInputRef}
          aria-label={`${pane.title} path`}
          value={pathDraft}
          onChange={(event) => setPathDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              submitPathEdit()
            } else if (event.key === 'Escape') {
              event.preventDefault()
              setEditingPath(false)
              setPathDraft(pane.path)
            }
          }}
          className="w-0 min-w-0 flex-1 select-text rounded-tab border border-accent-blue-border bg-light-panel px-2 py-1 font-mono text-row text-light-text outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:bg-dark-panel dark:text-dark-text"
        />
      ) : (
        <nav
          ref={setNavElement}
          aria-label={`${pane.title} path`}
          className="flex min-w-0 flex-1 items-center gap-1 overflow-hidden text-row"
          onDoubleClick={(event) => {
            if (event.target === event.currentTarget) {
              startPathEdit()
            }
          }}
        >
          {layout.collapsed ? (
            <span
              aria-hidden="true"
              data-testid="breadcrumb-collapse-marker"
              className="inline-flex shrink-0 items-center gap-1 text-row text-light-text-muted dark:text-dark-text-muted"
            >
              <span>{'..'}</span>
              <ChevronRightIcon className="h-3.5 w-3.5" />
            </span>
          ) : null}
          {layout.items.map((segment, index) => (
            <button
              key={segment.path}
              type="button"
              onClick={() => void navigatePane(pane.id, segment.path)}
              title={segment.truncated ? segment.fullLabel : undefined}
              className={`inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-tab px-2 py-1 text-row focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border ${
                index === layout.items.length - 1
                  ? 'bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
                  : 'text-light-text-soft hover:bg-light-hover dark:text-dark-text-soft dark:hover:bg-dark-hover'
              }`}
            >
              <span className="truncate">{segment.label}</span>
              {index < layout.items.length - 1 ? (
                <ChevronRightIcon className="h-3.5 w-3.5" />
              ) : null}
            </button>
          ))}
        </nav>
      )}
      <button
        type="button"
        aria-label="Navigate to ~"
        onClick={() => navigateShortcut('~')}
        className="inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-tab font-mono text-row text-light-text-soft hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
      >
        ~
      </button>
      <button
        type="button"
        aria-label="Navigate to /"
        onClick={() => navigateShortcut('/')}
        className="inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-tab font-mono text-row text-light-text-soft hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
      >
        /
      </button>
    </div>
  )
}

type Segment = { label: string; path: string }

/**
 * Splits an absolute path into clickable breadcrumb segments with correct
 * cumulative target paths, for both Windows (`C:\a\b`) and POSIX (`/a/b`).
 */
export function splitPath(path: string): Segment[] {
  if (!path) {
    return [{ label: '.', path: '.' }]
  }

  const displayPath = normalizeExtendedWindowsPath(path)

  const uncRoot = displayPath.match(/^\\\\([^\\/]+)[\\/]([^\\/]+)[\\/]?/)
  if (uncRoot) {
    const rootPath = `\\\\${uncRoot[1]}\\${uncRoot[2]}`
    const segments: Segment[] = [{ label: rootPath, path: rootPath }]
    const rest = displayPath.slice(uncRoot[0].length).split(/[\\/]/).filter(Boolean)
    let current = rootPath
    for (const part of rest) {
      current = `${current}\\${part}`
      segments.push({ label: part, path: current })
    }
    return segments
  }

  const windowsRoot = displayPath.match(/^([A-Za-z]:)[\\/]?/)
  if (windowsRoot) {
    const drive = windowsRoot[1]
    const rootPath = `${drive}\\`
    const segments: Segment[] = [{ label: drive, path: rootPath }]

    const rest = displayPath.slice(windowsRoot[0].length).split(/[\\/]/).filter(Boolean)
    let current = drive
    for (const part of rest) {
      current = `${current}\\${part}`
      segments.push({ label: part, path: current })
    }
    return segments
  }

  const parts = displayPath.split(/[\\/]/).filter(Boolean)
  const segments: Segment[] = [{ label: '/', path: '/' }]
  let current = ''
  for (const part of parts) {
    current = `${current}/${part}`
    segments.push({ label: part, path: current })
  }
  return segments
}

function normalizeExtendedWindowsPath(path: string) {
  if (path.toLowerCase().startsWith('\\\\?\\unc\\')) {
    return `\\\\${path.slice(8)}`
  }

  if (/^\\\\\?\\[A-Za-z]:[\\/]/.test(path)) {
    return path.slice(4)
  }

  return path
}
