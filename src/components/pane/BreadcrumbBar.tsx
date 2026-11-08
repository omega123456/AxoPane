import {
  useEffect,
  useReducer,
  useState,
  type FocusEvent,
  type MouseEvent,
  type RefObject,
} from 'react'
import type { PaneState } from '@/types/pane'
import type { VolumeInfo } from '@/lib/types/ipc'
import { ChevronRightIcon, FolderGlyph } from '@/components/icons'
import type { BreadcrumbLayoutMeasure, LayoutSegment } from '@/lib/breadcrumb-layout'
import { computeBreadcrumbLayout, createBreadcrumbMeasurer } from '@/lib/breadcrumb-layout'
import { useElementWidth } from '@/lib/use-element-width'
import { usePanesStore } from '@/stores/panes-store'
import { FOCUS_RING } from '@/lib/focus-ring'
import { normalizeWindowsCompatibilityPath } from '@/lib/path-compare'
import { TEXT_SECONDARY } from '@/lib/class-names'
import { handleKeyByTable } from '@/lib/key-table'
import { useSelectWhen } from '@/lib/use-focus-on-mount'

type BreadcrumbBarProps = {
  pane: PaneState
}

const NOT_YET_MEASURABLE: BreadcrumbLayoutMeasure = { segment: () => Number.NaN }

function useRerenderWhenFontsLoad() {
  const [, rerender] = useReducer((renderCount: number) => renderCount + 1, 0)

  useEffect(() => {
    let active = true
    const rerenderIfActive = () => {
      if (!active) {
        return
      }

      rerender()
    }

    const animationFrame = window.requestAnimationFrame(rerenderIfActive)
    const fonts = document.fonts
    if (!fonts) {
      return () => {
        active = false
        window.cancelAnimationFrame(animationFrame)
      }
    }

    void fonts.ready.then(rerenderIfActive)
    fonts.addEventListener?.('loadingdone', rerenderIfActive)

    return () => {
      active = false
      window.cancelAnimationFrame(animationFrame)
      fonts.removeEventListener?.('loadingdone', rerenderIfActive)
    }
  }, [])
}

function useBreadcrumbLayout(path: string, navElement: HTMLElement | null) {
  const volumes = usePanesStore((state) => state.volumes)
  const navWidth = useElementWidth(navElement)
  return computeBreadcrumbLayout({
    segments: withRootVolumeLabel(splitPath(path), volumes),
    availableWidth: navWidth,
    measure: navWidth > 0 ? createBreadcrumbMeasurer(navElement) : NOT_YET_MEASURABLE,
  })
}

function usePathEdit(pane: PaneState) {
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const [editingPath, setEditingPath] = useState(false)
  const [pathDraft, setPathDraft] = useState(pane.path)
  const pathInputRef = useSelectWhen(editingPath)

  return {
    editingPath,
    pathDraft,
    setPathDraft,
    pathInputRef,
    start: () => {
      setPathDraft(pane.path)
      setEditingPath(true)
    },
    submit: () => {
      const trimmed = pathDraft.trim()
      setEditingPath(false)
      if (trimmed && trimmed !== pane.path) {
        void navigatePane(pane.id, trimmed)
      }
    },
    cancel: () => {
      setEditingPath(false)
      setPathDraft(pane.path)
    },
  }
}

function isFocusMovingInside(event: FocusEvent<HTMLElement>) {
  return event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget)
}

export function BreadcrumbBar({ pane }: BreadcrumbBarProps) {
  const { pathInputRef, ...pathEdit } = usePathEdit(pane)
  useRerenderWhenFontsLoad()

  return (
    <div
      className="flex h-address shrink-0 items-center border-b border-light-divider px-0.75 dark:border-dark-divider"
      onBlur={(event) => {
        if (pathEdit.editingPath && !isFocusMovingInside(event)) {
          pathEdit.submit()
        }
      }}
      onContextMenu={(event) => {
        event.preventDefault()
        event.stopPropagation()
        pathEdit.start()
      }}
    >
      {pathEdit.editingPath ? (
        <PathEditor
          label={`${pane.title} path`}
          inputRef={pathInputRef}
          draft={pathEdit.pathDraft}
          onDraftChange={pathEdit.setPathDraft}
          onSubmit={pathEdit.submit}
          onCancel={pathEdit.cancel}
        />
      ) : (
        <BreadcrumbTrail pane={pane} onStartEdit={pathEdit.start} />
      )}
    </div>
  )
}

type PathEditorProps = {
  label: string
  inputRef: RefObject<HTMLInputElement | null>
  draft: string
  onDraftChange: (draft: string) => void
  onSubmit: () => void
  onCancel: () => void
}

function PathEditor({
  label,
  inputRef,
  draft,
  onDraftChange,
  onSubmit,
  onCancel,
}: PathEditorProps) {
  return (
    <div className="ml-0.75 flex h-8 min-w-0 flex-1 items-center rounded-control border border-b-2 border-light-control-stroke border-b-light-accent bg-light-card px-2 dark:border-dark-control-stroke dark:border-b-dark-accent dark:bg-dark-card">
      <input
        ref={inputRef}
        autoComplete="off"
        spellCheck={false}
        aria-label={label}
        value={draft}
        onChange={(event) => onDraftChange(event.target.value)}
        onKeyDown={(event) => handleKeyByTable(event, { Enter: onSubmit, Escape: onCancel })}
        className="w-0 min-w-0 flex-1 select-text bg-transparent font-mono text-row text-light-text outline-none selection:bg-light-accent selection:text-light-on-accent dark:text-dark-text dark:selection:bg-dark-accent dark:selection:text-dark-on-accent"
      />
    </div>
  )
}

function BreadcrumbTrail({ pane, onStartEdit }: { pane: PaneState; onStartEdit: () => void }) {
  const [navElement, setNavElement] = useState<HTMLElement | null>(null)
  const layout = useBreadcrumbLayout(pane.path, navElement)
  const startEditOnEmptyArea = (event: MouseEvent<HTMLElement>) => {
    if (event.target === event.currentTarget) {
      onStartEdit()
    }
  }

  return (
    // Flat at rest; the hover fill shows the row is clickable. Double-click
    // its empty part (or right-click anywhere) to type a path.
    <div
      className="ml-0.75 flex h-8 min-w-0 flex-1 cursor-text items-center overflow-hidden rounded-control pl-1.5 pr-1 hover:bg-light-hover dark:hover:bg-dark-hover"
      onDoubleClick={startEditOnEmptyArea}
    >
      <FolderGlyph className="mr-1 size-4 shrink-0" />
      <nav
        ref={setNavElement}
        aria-label={`${pane.title} path`}
        className="flex min-w-0 flex-1 items-center overflow-hidden whitespace-nowrap text-row"
        onDoubleClick={startEditOnEmptyArea}
      >
        {layout.collapsed ? (
          <span
            aria-hidden="true"
            data-testid="breadcrumb-collapse-marker"
            className={`inline-flex shrink-0 items-center ${TEXT_SECONDARY}`}
          >
            <span className="px-1.5">{'…'}</span>
            <CrumbSeparator />
          </span>
        ) : null}
        {layout.items.map((segment, index) => (
          <Crumb
            key={segment.path}
            paneId={pane.id}
            segment={segment}
            isCurrent={index === layout.items.length - 1}
          />
        ))}
      </nav>
    </div>
  )
}

function Crumb({
  paneId,
  segment,
  isCurrent,
}: {
  paneId: PaneState['id']
  segment: LayoutSegment
  isCurrent: boolean
}) {
  const navigatePane = usePanesStore((state) => state.navigatePane)
  return (
    <span className="inline-flex shrink-0 items-center">
      <button
        type="button"
        aria-current={isCurrent ? 'location' : undefined}
        onClick={() => void navigatePane(paneId, segment.path)}
        title={segment.truncated ? segment.fullLabel : undefined}
        className={`inline-flex shrink-0 items-center rounded-control px-1.5 py-1 text-row ${FOCUS_RING} ${
          isCurrent
            ? 'cursor-default font-semibold text-light-text dark:text-dark-text'
            : `cursor-pointer ${TEXT_SECONDARY} hover:bg-light-hover active:bg-light-press dark:hover:bg-dark-hover dark:active:bg-dark-press`
        }`}
      >
        <span className="truncate">{segment.label}</span>
      </button>
      {isCurrent ? null : <CrumbSeparator />}
    </span>
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

  const displayPath = normalizeWindowsCompatibilityPath(path)

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
