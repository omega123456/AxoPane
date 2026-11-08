import { useEffect, useEffectEvent, useMemo, useState, type ReactNode } from 'react'
import {
  CalendarIcon,
  ChevronDownIcon,
  DownloadIcon,
  FileTextIcon,
  KeyboardIcon,
  LayoutListIcon,
  PanelsTopLeftIcon,
  RotateCcwIcon,
  SettingsIcon,
  XIcon,
} from '@/components/icons'
import { Button, SelectionPill } from '@/components/controls'
import { captureShortcut, detectPlatformOs } from '@/lib/keymap'
import type { CommandId, LayoutConfig } from '@/lib/types/ipc'
import { useSettingsStore } from '@/stores/settings-store'
import { FOCUS_RING, FOCUS_RING_INSET } from '@/lib/focus-ring'
import { SCROLLBAR_THIN, TEXT_SECONDARY } from '@/lib/class-names'
import { useAppVersion } from '@/lib/use-app-version'
import { KeybindingsSection } from './settings/KeybindingsSection'
import { LayoutSection } from './settings/LayoutSection'
import {
  ColumnsSection,
  DatesSection,
  LogsSection,
  UpdatesSection,
} from './settings/SettingsSections'
import { SETTINGS_ICON_CLASS as iconClass, type SectionProps } from './settings/section-props'
import {
  cloneDraft,
  commitDraft,
  defaultDraft,
  shortcutConflictsByCommand,
  type DraftState,
  type DraftUpdater,
} from './settings/settings-draft'

type Section = 'layout' | 'columns' | 'keybindings' | 'dates' | 'updates' | 'logs'

const sectionNav: { key: Section; label: string; Icon: typeof SettingsIcon }[] = [
  { key: 'layout', label: 'View and layout', Icon: LayoutListIcon },
  { key: 'columns', label: 'Columns', Icon: PanelsTopLeftIcon },
  { key: 'dates', label: 'Dates', Icon: CalendarIcon },
  { key: 'keybindings', label: 'Keybindings', Icon: KeyboardIcon },
  { key: 'updates', label: 'Updates', Icon: DownloadIcon },
  { key: 'logs', label: 'Logs', Icon: FileTextIcon },
]

const sectionComponents: Record<Section, (props: SectionProps) => ReactNode> = {
  layout: LayoutSection,
  columns: ColumnsSection,
  dates: DatesSection,
  keybindings: KeybindingsSection,
  updates: UpdatesSection,
  logs: LogsSection,
}

export function SettingsModal() {
  const isOpen = useSettingsStore((state) => state.isOpen)
  if (!isOpen) {
    return null
  }

  return <SettingsModalContent />
}

function useSettingsDraft() {
  const [draft, setDraft] = useState<DraftState>(() => cloneDraft())

  const updateDraft: DraftUpdater = (updater) => {
    const next = updater(draft)
    setDraft(next)
    void commitDraft(next)
  }

  function updateLayout<K extends keyof LayoutConfig>(key: K, value: LayoutConfig[K]) {
    updateDraft((current) => ({ ...current, layout: { ...current.layout, [key]: value } }))
  }

  return { draft, updateDraft, updateLayout, resetDraft: () => updateDraft(defaultDraft) }
}

function useShortcutCapture(updateDraft: DraftUpdater) {
  const [capturing, setCapturing] = useState<CommandId | null>(null)

  const onCaptureKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (!capturing) {
      return
    }

    event.preventDefault()
    event.stopPropagation()

    const shortcutValue = captureShortcut(event)
    if (!shortcutValue) {
      return
    }

    setCapturing(null)
    updateDraft((current) => ({
      ...current,
      bindings: {
        ...current.bindings,
        [capturing]: [shortcutValue],
      },
    }))
  })

  useEffect(() => {
    if (!capturing) {
      return
    }

    window.addEventListener('keydown', onCaptureKeyDown, true)
    return () => window.removeEventListener('keydown', onCaptureKeyDown, true)
  }, [capturing])

  return { capturing, setCapturing }
}

function SettingsModalContent() {
  const section = useSettingsStore((state) => state.section)
  const close = useSettingsStore((state) => state.close)
  const { draft, updateDraft, updateLayout, resetDraft } = useSettingsDraft()
  const { capturing, setCapturing } = useShortcutCapture(updateDraft)
  const [search, setSearch] = useState('')
  const conflicts = useMemo(() => shortcutConflictsByCommand(draft.bindings), [draft.bindings])
  const SectionContent = sectionComponents[section]

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-scrim p-10">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        className="flex h-settings-h max-h-full w-settings max-w-full flex-col overflow-hidden rounded-overlay border border-light-window-stroke bg-light-base text-light-text shadow-light-dialog dark:border-dark-window-stroke dark:bg-dark-base dark:text-dark-text dark:shadow-dark-dialog"
      >
        <SettingsHeader onClose={close} />
        <div className="flex min-h-0 flex-1">
          <SettingsNav section={section} />
          <div className={`min-h-0 flex-1 overflow-auto pb-7 pl-3 pr-7 pt-1 ${SCROLLBAR_THIN}`}>
            <div className="mx-auto grid w-full max-w-settings-content content-start gap-1">
              <h2 className="mb-3.5 font-display text-title font-semibold">
                {sectionNav.find((item) => item.key === section)?.label}
              </h2>
              <SectionContent
                draft={draft}
                updateDraft={updateDraft}
                updateLayout={updateLayout}
                search={search}
                onSearchChange={setSearch}
                capturing={capturing}
                onCapture={setCapturing}
                conflicts={conflicts}
                platform={detectPlatformOs()}
              />
              <ResetExpander key={section} onReset={resetDraft} />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function SettingsHeader({ onClose }: { onClose: () => void }) {
  return (
    <header className="flex h-11 flex-none items-center gap-2.5 pl-4 pr-1.5 text-caption">
      <SettingsIcon className={iconClass} />
      Settings
      <Button
        variant="subtle"
        aria-label="Close settings"
        onClick={onClose}
        size="icon"
        className="ml-auto"
      >
        <XIcon className={iconClass} />
      </Button>
    </header>
  )
}

function SettingsNav({ section }: { section: Section }) {
  const open = useSettingsStore((state) => state.open)
  const appVersion = useAppVersion()
  return (
    <nav
      aria-label="Settings sections"
      className="flex w-settings-nav flex-none flex-col gap-0.5 px-2 pb-4 pt-1"
    >
      {sectionNav.map(({ key, label, Icon }) => {
        const active = key === section
        return (
          <button
            key={key}
            type="button"
            aria-current={active ? 'page' : undefined}
            onClick={() => open(key)}
            className={`relative flex h-8.5 flex-none cursor-pointer items-center gap-3 rounded-control px-3 text-body ${FOCUS_RING_INSET} ${
              active
                ? 'bg-light-hover dark:bg-dark-hover'
                : 'hover:bg-light-hover active:bg-light-press dark:hover:bg-dark-hover dark:active:bg-dark-press'
            }`}
          >
            {active ? <SelectionPill /> : null}
            <Icon className={iconClass} />
            {label}
          </button>
        )
      })}
      <div className="mt-6 px-3 text-caption text-light-text-tertiary dark:text-dark-text-tertiary">
        AxoPane {appVersion}
      </div>
    </nav>
  )
}

/**
 * "Reset to defaults" at the foot of every settings page: a card that expands
 * to confirm, so a stray click can't wipe the configuration.
 */
function ResetExpander({ onReset }: { onReset: () => void }) {
  const [expanded, setExpanded] = useState(false)
  return (
    <div className="mt-4.5 rounded-card border border-light-stroke bg-light-card dark:border-dark-stroke dark:bg-dark-card">
      <button
        type="button"
        aria-expanded={expanded}
        onClick={() => setExpanded((value) => !value)}
        className={`flex min-h-16 w-full cursor-pointer items-center gap-4 rounded-card px-4 py-2.5 text-left hover:bg-light-hover dark:hover:bg-dark-hover ${FOCUS_RING}`}
      >
        <RotateCcwIcon className={iconClass} />
        <span className="grid min-w-0 flex-1">
          <span className="text-body">Reset to defaults</span>
          <span className={`text-caption ${TEXT_SECONDARY}`}>
            Restore every setting to how AxoPane ships
          </span>
        </span>
        <ChevronDownIcon className={`size-3 ${TEXT_SECONDARY} ${expanded ? 'rotate-180' : ''}`} />
      </button>
      {expanded ? (
        <div className="flex items-center gap-4 border-t border-light-divider py-3 pl-12 pr-4 dark:border-dark-divider">
          <span className="flex-1 text-body">Settings change right away. You can't undo this.</span>
          <Button onClick={onReset}>Reset</Button>
        </div>
      ) : null}
    </div>
  )
}
