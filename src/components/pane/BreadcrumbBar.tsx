import { useEffect, useRef, useState, type ReactNode } from 'react'
import type { PaneState } from '@/types/pane'
import { ChevronRightIcon, PieChartIcon, RefreshIcon, SearchIcon } from '@/components/icons'
import type { BreadcrumbLayoutMeasure } from '@/lib/breadcrumb-layout'
import { computeBreadcrumbLayout, createBreadcrumbMeasurer } from '@/lib/breadcrumb-layout'
import { useElementWidth } from '@/lib/use-element-width'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useConfigStore } from '@/stores/config-store'
import { autoFolderSizeDisabledForPane, usePanesStore } from '@/stores/panes-store'

type BreadcrumbBarProps = {
  pane: PaneState
  isActive: boolean
  /** Reserved action slot for pane-scoped toolbar controls. Live FilePane deliberately leaves it empty until Phase 8. */
  actions?: ReactNode
}

export function BreadcrumbBar({ pane, isActive, actions }: BreadcrumbBarProps) {
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const setFilterDraft = usePanesStore((state) => state.setFilterDraft)
  const clearFilter = usePanesStore((state) => state.clearFilter)
  const refreshEverything = usePanesStore((state) => state.refreshEverything)
  const everythingAvailable = usePanesStore((state) => state.everythingStatus?.isAvailable ?? false)
  const autoFolderSize = useConfigStore((state) => state.autoFolderSize)
  const openActionDialog = useActionDialogStore((state) => state.open)
  // This pane has too many folders for eager auto-sizing, so surface the manual
  // button here regardless of the global setting (evaluated per pane).
  const tooManyFoldersForAuto = autoFolderSizeDisabledForPane(pane.entries)
  const showCalculateAllSizes = !everythingAvailable || !autoFolderSize || tooManyFoldersForAuto
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

  return (
    <div
      className="flex h-crumb items-center gap-2 border-b border-light-border bg-light-surface px-3 dark:border-dark-border dark:bg-dark-surface"
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
          onBlur={submitPathEdit}
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
          className="min-w-0 flex-1 select-text rounded-tab border border-accent-blue-border bg-light-panel px-2 py-1 font-mono text-row text-light-text outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:bg-dark-panel dark:text-dark-text"
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
              {index < layout.items.length - 1 ? <ChevronRightIcon className="h-3.5 w-3.5" /> : null}
            </button>
          ))}
        </nav>
      )}
      <span className="shrink-0 font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
        {pane.entries.length} items
      </span>
      <button
        type="button"
        aria-label={`Refresh ${pane.title}`}
        title="Refresh"
        onClick={() => void refreshEverything(pane.id)}
        className="inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-tab text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-soft dark:hover:bg-dark-hover"
      >
        <RefreshIcon className="h-3.5 w-3.5" />
      </button>
      {showCalculateAllSizes ? (
        <button
          type="button"
          aria-label={`Calculate all folder sizes in ${pane.title}`}
          title="Calculate all folder sizes"
          onClick={() => openActionDialog({ kind: 'calculateAllSizes', paneId: pane.id })}
          className="inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-tab text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-soft dark:hover:bg-dark-hover"
        >
          <PieChartIcon className="h-3.5 w-3.5" />
        </button>
      ) : null}
      {actions}
      <label
        className={`flex h-8 w-search items-center gap-2 rounded-tab border px-2 ${
          isActive ? 'border-accent-blue-border' : 'border-light-border dark:border-dark-border'
        } bg-light-panel dark:bg-dark-panel`}
      >
        <SearchIcon className="h-3.5 w-3.5 text-light-text-muted dark:text-dark-text-muted" />
        <input
          aria-label={`${pane.title} filter`}
          value={pane.filterDraft}
          onChange={(event) => setFilterDraft(pane.id, event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault()
              clearFilter(pane.id)
              // Hand keyboard focus back to the pane shell so arrow keys resume
              // driving the list directly instead of staying trapped in the
              // (now empty) filter input.
              document.querySelector<HTMLElement>(`[data-pane-id="${pane.id}"]`)?.focus()
            }
          }}
          placeholder="Filter current folder"
          className="min-w-0 flex-1 select-text bg-transparent text-row text-light-text outline-none placeholder:text-light-text-faint dark:text-dark-text dark:placeholder:text-dark-text-faint"
        />
      </label>
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
