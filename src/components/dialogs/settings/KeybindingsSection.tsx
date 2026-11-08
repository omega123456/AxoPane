import { AlertTriangleIcon, SearchIcon } from '@/components/icons'
import { Button } from '@/components/controls'
import {
  commandLabels,
  defaultKeymap,
  formatShortcutLabel,
  isReservedCommand,
  type PlatformOs,
} from '@/lib/keymap'
import type { CommandId } from '@/lib/types/ipc'
import { TEXT_SECONDARY } from '@/lib/class-names'
import { SETTINGS_ICON_CLASS as iconClass, type SectionProps } from './section-props'
import type { DraftUpdater } from './settings-draft'

export function KeybindingsSection(props: SectionProps) {
  const { search, onSearchChange } = props
  const filteredCommands = (Object.keys(commandLabels) as CommandId[]).filter((commandId) =>
    commandLabels[commandId].toLowerCase().includes(search.toLowerCase()),
  )
  return (
    <>
      <label className="mb-3 flex h-8 items-center gap-2 rounded-control border border-light-control-stroke border-b-light-boundary bg-light-control px-2.5 text-light-text-tertiary focus-within:border-b-2 focus-within:border-b-light-accent focus-within:bg-light-card dark:border-dark-control-stroke dark:border-b-dark-boundary dark:bg-dark-control dark:text-dark-text-tertiary dark:focus-within:border-b-dark-accent dark:focus-within:bg-dark-card">
        <SearchIcon className={iconClass} />
        <input
          aria-label="Search keybindings"
          placeholder="Search keybindings"
          autoComplete="off"
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          className="w-full select-text bg-transparent text-row text-light-text outline-none placeholder:text-light-text-tertiary dark:text-dark-text dark:placeholder:text-dark-text-tertiary"
        />
      </label>
      <div className="overflow-hidden rounded-card border border-light-stroke bg-light-card dark:border-dark-stroke dark:bg-dark-card">
        <table className="w-full table-fixed">
          <thead>
            <tr
              className={`border-b border-light-divider text-left text-caption ${TEXT_SECONDARY} dark:border-dark-divider`}
            >
              <th className="py-2 pl-4 font-normal">Command</th>
              <th className="w-44 py-2 font-normal">Shortcut</th>
              <th className="w-28 py-2 font-normal">Status</th>
              <th className="w-20 py-2 pr-4 text-right font-normal">Reset</th>
            </tr>
          </thead>
          <tbody className="text-row">
            {filteredCommands.map((commandId) => (
              <KeybindingRow key={commandId} commandId={commandId} {...props} />
            ))}
          </tbody>
        </table>
      </div>
    </>
  )
}

function KeybindingRow({ commandId, ...props }: SectionProps & { commandId: CommandId }) {
  const shortcut = props.draft.bindings[commandId][0] ?? ''
  return (
    <tr className="border-b border-light-divider last:border-b-0 dark:border-dark-divider">
      <td className="py-1.5 pl-4">{commandLabels[commandId]}</td>
      {isReservedCommand(commandId) ? (
        <ReservedShortcutCells shortcut={shortcut} platform={props.platform} />
      ) : (
        <EditableShortcutCells
          commandId={commandId}
          shortcut={shortcut}
          platform={props.platform}
          capturing={props.capturing === commandId}
          hasConflict={(props.conflicts.get(commandId)?.length ?? 0) > 0}
          onCapture={props.onCapture}
          updateDraft={props.updateDraft}
        />
      )}
    </tr>
  )
}

function ReservedShortcutCells({ shortcut, platform }: { shortcut: string; platform: PlatformOs }) {
  return (
    <>
      <td className="py-1.5">
        <span className={`inline-flex h-8 w-36 items-center px-3 font-mono ${TEXT_SECONDARY}`}>
          {formatShortcutLabel(shortcut, platform)}
        </span>
      </td>
      <td className={`py-1.5 ${TEXT_SECONDARY}`}>System default</td>
      <td className="py-1.5 pr-4" />
    </>
  )
}

type EditableShortcutCellsProps = {
  commandId: CommandId
  shortcut: string
  platform: PlatformOs
  capturing: boolean
  hasConflict: boolean
  onCapture: (commandId: CommandId) => void
  updateDraft: DraftUpdater
}

function shortcutButtonLabel(capturing: boolean, shortcut: string, platform: PlatformOs) {
  if (capturing) {
    return 'Press keys...'
  }
  if (!shortcut) {
    return 'Unassigned'
  }
  return formatShortcutLabel(shortcut, platform)
}

function EditableShortcutCells({
  commandId,
  shortcut,
  platform,
  capturing,
  hasConflict,
  onCapture,
  updateDraft,
}: EditableShortcutCellsProps) {
  const resetBinding = () =>
    updateDraft((current) => ({
      ...current,
      bindings: {
        ...current.bindings,
        [commandId]: [...defaultKeymap[commandId]],
      },
    }))
  return (
    <>
      <td className="py-1.5">
        <Button
          aria-label={`Capture ${commandLabels[commandId]} shortcut`}
          onClick={() => onCapture(commandId)}
          className="w-36 font-mono"
        >
          {shortcutButtonLabel(capturing, shortcut, platform)}
        </Button>
      </td>
      <td className={`py-1.5 ${TEXT_SECONDARY}`}>
        {hasConflict ? (
          <span className="inline-flex items-center gap-2 text-light-warning dark:text-dark-warning">
            <AlertTriangleIcon className={iconClass} />
            Conflict
          </span>
        ) : (
          'OK'
        )}
      </td>
      <td className="py-1.5 pr-4 text-right">
        <Button variant="subtle" onClick={resetBinding}>
          Reset
        </Button>
      </td>
    </>
  )
}
