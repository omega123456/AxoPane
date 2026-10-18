import { useEffect, useRef, useState } from 'react'
import type { PaneState } from '@/types/pane'
import type { VolumeInfo } from '@/lib/types/ipc'
import { ChevronRightIcon, FolderGlyph } from '@/components/icons'
import type { BreadcrumbLayoutMeasure } from '@/lib/breadcrumb-layout'
import { computeBreadcrumbLayout, createBreadcrumbMeasurer } from '@/lib/breadcrumb-layout'
import { useElementWidth } from '@/lib/use-element-width'
import { usePanesStore } from '@/stores/panes-store'
import { FOCUS_RING } from '@/lib/focus-ring'

type BreadcrumbBarProps = {
  pane: PaneState
}

export function BreadcrumbBar({ pane }: BreadcrumbBarProps) {
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const volumes = usePanesStore((state) => state.volumes)
  const [editingPath, setEditingPath] = useState(false)
  const [pathDraft, setPathDraft] = useState(pane.path)
  const pathInputRef = useRef<HTMLInputElement>(null)
  const [navElement, setNavElement] = useState<HTMLElement | null>(null)
  const [measureVersion, setMeasureVersion] = useState(0)

  const segments = withRootVolumeLabel(splitPath(pane.path), volumes)
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
      className="flex h-address shrink-0 items-center border-b border-light-divider px-0.75 dark:border-dark-divider"
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
        <div className="ml-0.75 flex h-8 min-w-0 flex-1 items-center rounded-control border border-b-2 border-light-control-stroke border-b-light-accent bg-light-card px-2 dark:border-dark-control-stroke dark:border-b-dark-accent dark:bg-dark-card">
          <input
            ref={pathInputRef}
            autoComplete="off"
            spellCheck={false}
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
            className="w-0 min-w-0 flex-1 select-text bg-transparent font-mono text-row text-light-text outline-none selection:bg-light-accent selection:text-light-on-accent dark:text-dark-text dark:selection:bg-dark-accent dark:selection:text-dark-on-accent"
          />
        </div>
      ) : (
        // Flat at rest; the hover fill shows the row is clickable. Double-click
        // its empty part (or right-click anywhere) to type a path.
        <div
          className="ml-0.75 flex h-8 min-w-0 flex-1 cursor-text items-center overflow-hidden rounded-control pl-1.5 pr-1 hover:bg-light-hover dark:hover:bg-dark-hover"
          onDoubleClick={(event) => {
            if (event.target === event.currentTarget) {
              startPathEdit()
            }
          }}
        >
          <FolderGlyph className="mr-1 size-4 shrink-0" />
          <nav
            ref={setNavElement}
            aria-label={`${pane.title} path`}
            className="flex min-w-0 flex-1 items-center overflow-hidden whitespace-nowrap text-row"
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
                className="inline-flex shrink-0 items-center text-light-text-secondary dark:text-dark-text-secondary"
              >
                <span className="px-1.5">{'…'}</span>
                <CrumbSeparator />
              </span>
            ) : null}
            {layout.items.map((segment, index) => {
              const isCurrent = index === layout.items.length - 1
              return (
                <span key={segment.path} className="inline-flex shrink-0 items-center">
                  <button
                    type="button"
                    aria-current={isCurrent ? 'location' : undefined}
                    onClick={() => void navigatePane(pane.id, segment.path)}
                    title={segment.truncated ? segment.fullLabel : undefined}
                    className={`inline-flex shrink-0 items-center rounded-control px-1.5 py-1 text-row ${FOCUS_RING} ${
                      isCurrent
                        ? 'cursor-default font-semibold text-light-text dark:text-dark-text'
                        : 'cursor-pointer text-light-text-secondary hover:bg-light-hover active:bg-light-press dark:text-dark-text-secondary dark:hover:bg-dark-hover dark:active:bg-dark-press'
                    }`}
                  >
                    <span className="truncate">{segment.label}</span>
                  </button>
                  {isCurrent ? null : <CrumbSeparator />}
                </span>
              )
            })}
          </nav>
        </div>
      )}
    </div>
  )
}

function CrumbSeparator() {
  return (
    <span
      aria-hidden="true"
      className="flex shrink-0 items-center text-light-text-tertiary dark:text-dark-text-tertiary"
    >
      <ChevronRightIcon className="size-3" />
    </span>
  )
}

type Segment = { label: string; path: string }

/**
 * Labels the POSIX root crumb with the boot volume's name ("Macintosh HD"), as
 * Finder does. Drive letters and UNC roots already name their volume.
 */
export function withRootVolumeLabel(segments: Segment[], volumes: VolumeInfo[]): Segment[] {
  const [root, ...rest] = segments
  if (root?.path !== '/') {
    return segments
  }
  const label = volumes.find((volume) => volume.mountRoot === '/')?.label.trim()
  return label ? [{ ...root, label }, ...rest] : segments
}

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
