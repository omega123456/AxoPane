import type { ThemeMode } from '@/stores/theme-store'
import { persistAppConfig } from '@/lib/app-config'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import {
  ArrowLeftIcon,
  ArrowUpIcon,
  ChevronDownIcon,
  EyeIcon,
  EyeOffIcon,
  MoonStarIcon,
  PanelLeftIcon,
  RefreshIcon,
  SearchIcon,
  SettingsIcon,
  SunIcon,
} from '@/components/icons'

type CommandBarProps = {
  theme: ThemeMode
  setTheme: (theme: ThemeMode) => void
}

export function CommandBar({ theme, setTheme }: CommandBarProps) {
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const pane = usePanesStore((state) => state.panes[activePaneId])
  const reloadPane = usePanesStore((state) => state.reloadPane)
  const goUp = usePanesStore((state) => state.goUp)
  const showHiddenFiles = usePanesStore((state) => state.showHiddenFiles)
  const setShowHiddenFiles = usePanesStore((state) => state.setShowHiddenFiles)
  const detailsVisible = useLayoutStore((state) => state.detailsVisible)
  const setDetailsVisible = useLayoutStore((state) => state.setDetailsVisible)
  const openSettings = useSettingsStore((state) => state.open)

  return (
    <header className="flex h-command items-center gap-1 border-b border-light-border bg-light-menubar px-2 dark:border-dark-border dark:bg-dark-menubar">
      <ToolbarButton label="Back" disabled>
        <ArrowLeftIcon className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton label="Up" onClick={() => void goUp(activePaneId)}>
        <ArrowUpIcon className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton label="Refresh" onClick={() => void reloadPane(activePaneId)}>
        <RefreshIcon className="h-4 w-4" />
      </ToolbarButton>
      <div className="mx-2 h-5 w-px bg-light-border dark:bg-dark-border" />
      <button
        type="button"
        className="inline-flex h-8 items-center gap-2 rounded-tab bg-accent-blue-soft px-3 text-row font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
      >
        <SearchIcon className="h-4 w-4" />
        <span className="truncate">{pane.filterApplied ? `Filter: ${pane.filterApplied}` : pane.path}</span>
        <ChevronDownIcon className="h-4 w-4" />
      </button>
      <div className="ml-auto flex items-center gap-2">
        <button
          type="button"
          aria-pressed={showHiddenFiles}
          onClick={() => void setShowHiddenFiles(!showHiddenFiles)}
          className={`inline-flex h-8 items-center gap-2 rounded-tab border px-3 text-row focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border ${
            showHiddenFiles
              ? 'border-accent-blue-border bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
              : 'border-light-border bg-light-surface text-light-text-soft dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft'
          }`}
        >
          {showHiddenFiles ? <EyeIcon className="h-4 w-4" /> : <EyeOffIcon className="h-4 w-4" />}
          Hidden files
        </button>
        <button
          type="button"
          onClick={() => {
            setDetailsVisible(!detailsVisible)
            void persistAppConfig()
          }}
          className="inline-flex h-8 items-center gap-2 rounded-tab border border-light-border bg-light-surface px-3 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft"
        >
          <PanelLeftIcon className="h-4 w-4" />
          {detailsVisible ? 'Hide details' : 'Show details'}
        </button>
        <button
          type="button"
          onClick={() => openSettings('keybindings')}
          className="inline-flex h-8 items-center gap-2 rounded-tab border border-light-border bg-light-surface px-3 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft"
        >
          <SettingsIcon className="h-4 w-4" />
          Settings
        </button>
        <button
          type="button"
          onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
          className="inline-flex h-8 items-center gap-2 rounded-tab border border-light-border bg-light-surface px-3 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft"
        >
          {theme === 'dark' ? <SunIcon className="h-4 w-4" /> : <MoonStarIcon className="h-4 w-4" />}
          <span>{theme === 'dark' ? 'Light theme' : 'Dark theme'}</span>
        </button>
      </div>
    </header>
  )
}

type ToolbarButtonProps = {
  children: ReactNode
  disabled?: boolean
  label: string
  onClick?: () => void
}

import type { ReactNode } from 'react'

function ToolbarButton({ children, disabled = false, label, onClick }: ToolbarButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
      className="inline-flex h-8 w-8 items-center justify-center rounded-tab text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border disabled:opacity-40 dark:text-dark-text-soft hover:bg-light-hover dark:hover:bg-dark-hover"
    >
      {children}
    </button>
  )
}
