import {
  CalendarClockIcon,
  CalendarIcon,
  ClockIcon,
  GripVerticalIcon,
  ListFilterIcon,
  TimerIcon,
} from '@/components/icons'
import { SectionLabel, SelectField, SettingRow, ToggleSwitch } from '@/components/controls'
import { UpdatesSettings } from '@/components/dialogs/UpdatesSettings'
import { LogViewer } from '@/components/dialogs/LogViewer'
import { columnDefinitions } from '@/lib/columns'
import { DATE_FORMATS, type DateFormat, dateFormatLabels } from '@/lib/date-format'
import type { ColumnConfig, LogLevel } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { moveItem } from '@/lib/collections'
import { TEXT_SECONDARY } from '@/lib/class-names'
import { SETTINGS_ICON_CLASS as iconClass, type SectionProps } from './section-props'
import type { DraftState } from './settings-draft'

const COLUMN_DRAG_FORMAT = 'text/column'

const DATE_FORMAT_OPTIONS: { value: DateFormat; label: string }[] = DATE_FORMATS.map((format) => ({
  value: format,
  label: dateFormatLabels[format],
}))

const LOG_LEVEL_OPTIONS: { value: LogLevel; label: string }[] = [
  { value: 'error', label: 'Error' },
  { value: 'warn', label: 'Warn' },
  { value: 'info', label: 'Info' },
  { value: 'debug', label: 'Debug' },
  { value: 'trace', label: 'Trace' },
]

function withColumnMoved(
  current: DraftState,
  fromKey: ColumnConfig['key'],
  toKey: ColumnConfig['key'],
): DraftState {
  const { columns } = current
  const fromIndex = columns.findIndex((item) => item.key === fromKey)
  const toIndex = columns.findIndex((item) => item.key === toKey)
  const columnMissing = fromIndex === -1 || toIndex === -1
  if (columnMissing) {
    return current
  }
  return { ...current, columns: moveItem(columns, fromIndex, toIndex) }
}

function withColumnVisibilityToggled(current: DraftState, key: ColumnConfig['key']): DraftState {
  return {
    ...current,
    columns: current.columns.map((item) =>
      item.key === key ? { ...item, visible: !item.visible } : item,
    ),
  }
}

export function ColumnsSection({ draft, updateDraft }: SectionProps) {
  return (
    <>
      <ul className="grid gap-1">
        {draft.columns.map((column) => (
          <li
            key={column.key}
            draggable
            onDragStart={(event) => event.dataTransfer.setData(COLUMN_DRAG_FORMAT, column.key)}
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
              event.preventDefault()
              const fromKey = event.dataTransfer.getData(COLUMN_DRAG_FORMAT) as ColumnConfig['key']
              updateDraft((current) => withColumnMoved(current, fromKey, column.key))
            }}
            className="flex min-h-12 cursor-grab items-center justify-between gap-4 rounded-card border border-light-stroke bg-light-card px-4 py-2 dark:border-dark-stroke dark:bg-dark-card"
          >
            <span className="inline-flex items-center gap-4 text-body">
              <GripVerticalIcon className={`size-4 ${TEXT_SECONDARY}`} />
              {columnDefinitions[column.key].label}
            </span>
            <ToggleSwitch
              label={`${columnDefinitions[column.key].label} column`}
              checked={column.visible}
              onChange={() =>
                updateDraft((current) => withColumnVisibilityToggled(current, column.key))
              }
            />
          </li>
        ))}
      </ul>
    </>
  )
}

export function DatesSection({ draft, updateDraft }: SectionProps) {
  return (
    <>
      <SectionLabel>Date display</SectionLabel>
      <SettingRow
        icon={<CalendarIcon className={iconClass} />}
        title="Date format"
        description="How modified and created dates are written in lists and properties"
        control={
          <SelectField
            ariaLabel="Date format"
            value={draft.dateFormat}
            onChange={(value) => updateDraft((current) => ({ ...current, dateFormat: value }))}
            options={DATE_FORMAT_OPTIONS}
          />
        }
      />
      <SettingRow
        icon={<ClockIcon className={iconClass} />}
        title="Show time"
        description="Append the time of day (HH:MM) to the date"
        control={
          <ToggleSwitch
            label="Show time"
            checked={draft.showTime}
            onChange={(value) => updateDraft((current) => ({ ...current, showTime: value }))}
          />
        }
      />
      <SettingRow
        icon={<TimerIcon className={iconClass} />}
        title="Show seconds"
        description="Include seconds (HH:MM:SS) when the time is shown"
        control={
          <ToggleSwitch
            label="Show seconds"
            checked={draft.showSeconds}
            onChange={(value) => updateDraft((current) => ({ ...current, showSeconds: value }))}
          />
        }
      />
      <SettingRow
        icon={<CalendarClockIcon className={iconClass} />}
        title="Relative dates"
        description="Show colour-coded phrases like “15 minutes ago” for items changed within the last 2 days, then fall back to the format above"
        control={
          <ToggleSwitch
            label="Relative dates"
            checked={draft.relativeDates}
            onChange={(value) => updateDraft((current) => ({ ...current, relativeDates: value }))}
          />
        }
      />
    </>
  )
}

export function UpdatesSection({ draft, updateDraft }: SectionProps) {
  return (
    <>
      <UpdatesSettings
        value={draft.updateCheckInterval}
        onChange={(value) => updateDraft((current) => ({ ...current, updateCheckInterval: value }))}
      />
    </>
  )
}

export function LogsSection() {
  const logLevel = useConfigStore((state) => state.logLevel)
  const setLogLevel = useConfigStore((state) => state.setLogLevel)
  return (
    <>
      <SectionLabel>Logging</SectionLabel>
      <SettingRow
        icon={<ListFilterIcon className={iconClass} />}
        title="Capture level"
        description="Minimum severity written to the daily log file"
        control={
          <SelectField
            ariaLabel="Capture level"
            value={logLevel}
            onChange={(value) => void setLogLevel(value)}
            options={LOG_LEVEL_OPTIONS}
          />
        }
      />
      <LogViewer />
    </>
  )
}
