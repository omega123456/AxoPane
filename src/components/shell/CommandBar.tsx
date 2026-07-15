import type { ThemeMode } from '@/stores/theme-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import {
  EyeIcon,
  EyeOffIcon,
  MoonStarIcon,
  SettingsIcon,
  SunIcon,
} from '@/components/icons'

type CommandBarProps = {
  theme: ThemeMode
  setTheme: (theme: ThemeMode) => void
}

export function CommandBar({ theme, setTheme }: CommandBarProps) {
  const showHiddenFiles = usePanesStore((state) => state.showHiddenFiles)
  const setShowHiddenFiles = usePanesStore((state) => state.setShowHiddenFiles)
  const openSettings = useSettingsStore((state) => state.open)

  return (
    <header className="flex h-command items-center gap-1 border-b border-light-border bg-light-menubar px-2 dark:border-dark-border dark:bg-dark-menubar">
      <div className="ml-auto flex items-center gap-2">
        <button
          type="button"
          aria-pressed={showHiddenFiles}
          onClick={() => void setShowHiddenFiles(!showHiddenFiles)}
          className={`inline-flex h-8 cursor-pointer items-center gap-2 rounded-tab border px-3 text-row focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border ${
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
          onClick={() => openSettings()}
          className="inline-flex h-8 cursor-pointer items-center gap-2 rounded-tab border border-light-border bg-light-surface px-3 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft"
        >
          <SettingsIcon className="h-4 w-4" />
          Settings
        </button>
        <button
          type="button"
          onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
          className="inline-flex h-8 cursor-pointer items-center gap-2 rounded-tab border border-light-border bg-light-surface px-3 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft"
        >
          {theme === 'dark' ? (
            <SunIcon className="h-4 w-4" />
          ) : (
            <MoonStarIcon className="h-4 w-4" />
          )}
          <span>{theme === 'dark' ? 'Light theme' : 'Dark theme'}</span>
        </button>
      </div>
    </header>
  )
}
