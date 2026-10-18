import type { ReactNode } from 'react'
import type { ThemeMode } from '@/stores/theme-store'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useConfigStore } from '@/stores/config-store'
import { autoFolderSizeDisabledForPane, usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import { tabLabel } from '@/stores/tabs-store'
import {
  ArrowLeftIcon,
  ArrowUpIcon,
  EyeOffIcon,
  FilePlusIcon,
  FolderPlusIcon,
  MoonStarIcon,
  PieChartIcon,
  RefreshIcon,
  SearchIcon,
  SettingsIcon,
  SunIcon,
} from '@/components/icons'
import { PaneViewMenu } from '@/components/pane/PaneViewMenu'
import { FOCUS_RING } from '@/lib/focus-ring'
import { forwardPaneFilterKey } from '@/lib/pane-filter-keys'

type CommandBarProps = {
  theme: ThemeMode
  setTheme: (theme: ThemeMode) => void
}

const BUTTON_CLASS_NAME = `inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-control hover:bg-light-hover active:bg-light-press disabled:cursor-default disabled:bg-transparent disabled:text-light-text-disabled dark:hover:bg-dark-hover dark:active:bg-dark-press dark:disabled:text-dark-text-disabled ${FOCUS_RING}`

function CommandButton({
  label,
  title,
  onClick,
  disabled,
  pressed,
  className = 'text-light-text dark:text-dark-text',
  children,
}: {
  label: string
  title: string
  onClick: () => void
  disabled?: boolean
  pressed?: boolean
  className?: string
  children: ReactNode
}) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={pressed}
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={`${BUTTON_CLASS_NAME} ${className}`}
    >
      {children}
    </button>
  )
}

function Separator() {
  return (
    <span
      aria-hidden="true"
      className="mx-1.5 h-5 w-px shrink-0 bg-light-divider dark:bg-dark-divider"
    />
  )
}

/**
 * The one command bar over both panes. Every command acts on the active pane,
 * and the single filter field edits that pane's own filter text.
 */
export function CommandBar({ theme, setTheme }: CommandBarProps) {
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const pane = usePanesStore((state) => state.panes[state.activePaneId])
  const canGoBack = pane.historyIndex > 0
  const goBack = usePanesStore((state) => state.goBack)
  const goUp = usePanesStore((state) => state.goUp)
  const refreshSizes = usePanesStore((state) => state.refreshSizes)
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const setFilterDraft = usePanesStore((state) => state.setFilterDraft)
  const clearFilter = usePanesStore((state) => state.clearFilter)
  const showHiddenFiles = usePanesStore((state) => state.showHiddenFiles)
  const setShowHiddenFiles = usePanesStore((state) => state.setShowHiddenFiles)
  const autoSizingAvailable = usePanesStore(
    (state) => state.folderSizeIndexStatus?.isAvailable ?? false,
  )
  // Folders automatic sizing could not answer: outside the macOS index or an
  // unvisited protected folder (FR8.3).
  const hasUnknownFolder = usePanesStore((state) =>
    pane.entries.some((entry) => entry.isDir && state.sizeStates[entry.path]?.state === 'unknown'),
  )
  const autoFolderSize = useConfigStore((state) => state.autoFolderSize)
  const openActionDialog = useActionDialogStore((state) => state.open)
  const openSettings = useSettingsStore((state) => state.open)
  const showCalculateAllSizes =
    !autoSizingAvailable ||
    !autoFolderSize ||
    autoFolderSizeDisabledForPane(pane.entries) ||
    hasUnknownFolder
  const folderName = tabLabel(pane)

  return (
    <div
      role="toolbar"
      aria-label="Command bar"
      className="flex h-command min-w-0 items-center gap-1 border-x border-b border-light-stroke bg-light-layer px-0.75 dark:border-dark-stroke dark:bg-dark-layer"
    >
      <CommandButton
        label={`Back in ${pane.title}`}
        title="Back"
        disabled={!canGoBack}
        onClick={() => void goBack(activePaneId)}
      >
        <ArrowLeftIcon className="size-4" />
      </CommandButton>
      <CommandButton
        label={`Up in ${pane.title}`}
        title="Up"
        onClick={() => void goUp(activePaneId)}
      >
        <ArrowUpIcon className="size-4" />
      </CommandButton>
      <CommandButton
        label={`Refresh ${pane.title}`}
        title="Refresh"
        onClick={() => void refreshSizes(activePaneId)}
      >
        <RefreshIcon className="size-4" />
      </CommandButton>
      <CommandButton
        label="Navigate to ~"
        title="Navigate to ~"
        onClick={() => void navigatePane(activePaneId, '~')}
        className="font-mono text-body text-light-text-secondary dark:text-dark-text-secondary"
      >
        ~
      </CommandButton>
      <CommandButton
        label="Navigate to /"
        title="Navigate to /"
        onClick={() => void navigatePane(activePaneId, '/')}
        className="font-mono text-body text-light-text-secondary dark:text-dark-text-secondary"
      >
        /
      </CommandButton>
      <Separator />
      <CommandButton
        label={`Create new folder in ${pane.title}`}
        title="Create new folder"
        onClick={() => openActionDialog({ kind: 'newFolder', paneId: activePaneId })}
      >
        <FolderPlusIcon className="size-4" />
      </CommandButton>
      <CommandButton
        label={`Create new file in ${pane.title}`}
        title="Create new file"
        onClick={() => openActionDialog({ kind: 'newFile', paneId: activePaneId })}
      >
        <FilePlusIcon className="size-4" />
      </CommandButton>
      {showCalculateAllSizes ? (
        <CommandButton
          label={`Calculate all folder sizes in ${pane.title}`}
          title="Calculate all folder sizes"
          onClick={() => openActionDialog({ kind: 'calculateAllSizes', paneId: activePaneId })}
        >
          <PieChartIcon className="size-4" />
        </CommandButton>
      ) : null}
      <Separator />
      <PaneViewMenu paneId={activePaneId} />
      <span className="flex-1" />
      <CommandButton
        label="Hidden files"
        title={showHiddenFiles ? 'Hidden files (on)' : 'Hidden files (off)'}
        pressed={showHiddenFiles}
        onClick={() => void setShowHiddenFiles(!showHiddenFiles)}
        className={
          showHiddenFiles
            ? 'bg-light-hover text-light-accent dark:bg-dark-hover dark:text-dark-accent'
            : 'text-light-text dark:text-dark-text'
        }
      >
        <EyeOffIcon className="size-4" />
      </CommandButton>
      <CommandButton label="Settings" title="Settings" onClick={() => openSettings()}>
        <SettingsIcon className="size-4" />
      </CommandButton>
      <CommandButton
        label={theme === 'dark' ? 'Light theme' : 'Dark theme'}
        title={theme === 'dark' ? 'Light theme' : 'Dark theme'}
        onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
      >
        {theme === 'dark' ? <SunIcon className="size-4" /> : <MoonStarIcon className="size-4" />}
      </CommandButton>
      <label
        title="Filters the active pane"
        className="ml-1.5 flex h-7 w-search shrink-0 items-center gap-1.5 rounded-control border border-light-control-stroke border-b-light-control-stroke-bottom bg-light-control px-2 text-light-text-tertiary focus-within:border-b-light-accent focus-within:bg-light-card dark:border-dark-control-stroke dark:border-b-dark-control-stroke-bottom dark:bg-dark-control dark:text-dark-text-tertiary dark:focus-within:border-b-dark-accent dark:focus-within:bg-dark-card"
      >
        <SearchIcon className="size-4 shrink-0" />
        <input
          data-filter-input=""
          aria-label={`${pane.title} filter`}
          autoComplete="off"
          spellCheck={false}
          value={pane.filterDraft}
          onChange={(event) => setFilterDraft(activePaneId, event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault()
              clearFilter(activePaneId)
              document.querySelector<HTMLElement>(`[data-pane-id="${activePaneId}"]`)?.focus()
              return
            }
            forwardPaneFilterKey(activePaneId, event)
          }}
          placeholder={`Filter ${folderName}`}
          className="min-w-0 flex-1 select-text bg-transparent text-row text-light-text outline-none placeholder:text-light-text-tertiary dark:text-dark-text dark:placeholder:text-dark-text-tertiary"
        />
      </label>
    </div>
  )
}
