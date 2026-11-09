import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
} from 'react'
import { Button } from '@/components/controls'
import { DialogShell } from '@/components/dialogs/DialogShell'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { LoaderCircleIcon } from '@/components/icons'
import {
  canSetDefaultApplication,
  getDefaultApplication,
  listApplications,
} from '@/lib/app-picker-commands'
import { dateToneClassName, type DateFormat, formatEntryDate } from '@/lib/date-format'
import { formatBytes, formatCount } from '@/lib/format'
import { useConfigStore } from '@/stores/config-store'
import { useDefaultAppDialogStore } from '@/stores/default-app-dialog-store'
import {
  usePropertiesDialogStore,
  type PropertiesDialogItem,
  type PropertiesDialogState,
} from '@/stores/properties-dialog-store'
import type { MacApp } from '@/lib/types/ipc'
import { NO_CLASS, SCROLLBAR_THIN, TEXT_SECONDARY } from '@/lib/class-names'
import { handleKeyByTable } from '@/lib/key-table'

function formatItemSize(sizeBytes: number | null, itemCount: number | null, isDir: boolean) {
  if (typeof sizeBytes === 'number') {
    return formatBytes(sizeBytes)
  }

  return formatContains(itemCount, isDir)
}

function formatContains(itemCount: number | null, isDir: boolean) {
  if (!isDir) {
    return '—'
  }
  if (typeof itemCount !== 'number') {
    return '—'
  }

  return `${formatCount(itemCount)} ${itemCount === 1 ? 'item' : 'items'}`
}

type DefaultAppState = {
  app: MacApp | null
  loaded: boolean
  isLoadingPicker: boolean
}

const INITIAL_DEFAULT_APP_STATE: DefaultAppState = {
  app: null,
  loaded: false,
  isLoadingPicker: false,
}

function withLoadedDefaultApp(app: MacApp | null) {
  return (current: DefaultAppState): DefaultAppState => ({ ...current, app, loaded: true })
}

function withPickerLoading(isLoadingPicker: boolean) {
  return (current: DefaultAppState): DefaultAppState => ({ ...current, isLoadingPicker })
}

function useDefaultApplication(dialog: PropertiesDialogState | null) {
  const closeRef = useRef<HTMLButtonElement>(null)
  const defaultAppPickerGuardRef = useRef<{ cancelled: boolean } | null>(null)
  const defaultAppDialogClosed = useDefaultAppDialogStore((state) => state.dialog === null)
  const [defaultApp, setDefaultApp] = useState(INITIAL_DEFAULT_APP_STATE)
  const [prevDialogForDefaultApp, setPrevDialogForDefaultApp] =
    useState<PropertiesDialogState | null>(dialog)

  // Reset render-derived state synchronously when Properties re-opens for a
  // different selection, rather than in an effect (avoids a cascading-render
  // setState-in-effect, since this only fires when `dialog` identity changes).
  if (dialog !== prevDialogForDefaultApp) {
    setPrevDialogForDefaultApp(dialog)
    setDefaultApp(INITIAL_DEFAULT_APP_STATE)
  }

  useEffect(() => {
    if (!dialog) {
      return
    }

    closeRef.current?.focus()

    // Cancel any in-flight "Set Default Application…" app-list fetch when
    // Properties closes or re-opens for a different selection, matching the
    // cancelled-flag pattern used below for the default-app lookup.
    return () => {
      if (defaultAppPickerGuardRef.current) {
        defaultAppPickerGuardRef.current.cancelled = true
      }
    }
  }, [dialog])

  useEffect(() => {
    if (!dialog) {
      return
    }
    if (!canSetDefaultApplication(dialog.items)) {
      return
    }

    let cancelled = false
    void getDefaultApplication(dialog.items[0].path).then((app) => {
      if (cancelled) {
        return
      }
      setDefaultApp(withLoadedDefaultApp(app))
    })

    return () => {
      cancelled = true
    }
    // Re-run when the Set Default Application picker closes so a freshly
    // chosen app is reflected here without requiring the user to reopen
    // Properties.
  }, [dialog, defaultAppDialogClosed])

  async function openDefaultAppDialog(item: PropertiesDialogItem) {
    const guard = { cancelled: false }
    defaultAppPickerGuardRef.current = guard

    setDefaultApp(withPickerLoading(true))
    const apps = await listApplications()

    if (guard.cancelled) {
      return
    }

    setDefaultApp(withPickerLoading(false))
    useDefaultAppDialogStore.getState().open({ filePath: item.path, fileName: item.name, apps })
  }

  return { closeRef, defaultApp, openDefaultAppDialog }
}

function propertiesSummary(dialog: PropertiesDialogState | null) {
  if (!dialog) {
    return null
  }

  const itemCount = dialog.items.length
  const isSingleItem = itemCount === 1
  const singleItem = isSingleItem ? dialog.items[0] : null
  return {
    heading: singleItem ? singleItem.name : `${formatCount(itemCount)} items selected`,
    itemCountLabel: `${formatCount(itemCount)} ${isSingleItem ? 'item' : 'items'}`,
    singleItem,
  }
}

export function PropertiesDialog() {
  const dialog = usePropertiesDialogStore((state) => state.dialog)
  const close = usePropertiesDialogStore((state) => state.close)
  const dateFormat = useConfigStore((state) => state.dateFormat)
  const showTime = useConfigStore((state) => state.showTime)
  const showSeconds = useConfigStore((state) => state.showSeconds)
  const defaultApplication = useDefaultApplication(dialog)
  const summary = useMemo(() => propertiesSummary(dialog), [dialog])

  if (!dialog) {
    return null
  }
  if (!summary) {
    return null
  }

  const singleItem = summary.singleItem

  function onKeyDown(event: KeyboardEvent) {
    handleKeyByTable(event, { Escape: close, Enter: close })
  }

  const canSetDefault = canSetDefaultApplication(dialog.items)
  return (
    <DialogShell
      label="Properties"
      title={summary.heading}
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <PropertiesFooter
          canSetDefault={canSetDefault}
          isLoadingPicker={defaultApplication.defaultApp.isLoadingPicker}
          closeRef={defaultApplication.closeRef}
          onSetDefault={() =>
            singleItem && void defaultApplication.openDefaultAppDialog(singleItem)
          }
          onClose={close}
        />
      }
    >
      <p className={`-mt-2 text-caption ${TEXT_SECONDARY}`}>{summary.itemCountLabel}</p>
      {singleItem ? (
        <SingleItemProperties
          item={singleItem}
          defaultAppRow={
            canSetDefault ? <DefaultAppRow defaultApp={defaultApplication.defaultApp} /> : null
          }
          dateDisplay={{ dateFormat, showTime, showSeconds }}
        />
      ) : (
        <SelectedPathsList items={dialog.items} />
      )}
    </DialogShell>
  )
}

type PropertiesFooterProps = {
  canSetDefault: boolean
  isLoadingPicker: boolean
  closeRef: Ref<HTMLButtonElement>
  onSetDefault: () => void
  onClose: () => void
}

function PropertiesFooter({
  canSetDefault,
  isLoadingPicker,
  closeRef,
  onSetDefault,
  onClose,
}: PropertiesFooterProps) {
  return (
    <>
      {canSetDefault ? (
        <Button disabled={isLoadingPicker} onClick={onSetDefault}>
          {isLoadingPicker ? (
            <>
              <LoaderCircleIcon className="size-4 animate-spin" />
              Loading…
            </>
          ) : (
            'Set Default Application…'
          )}
        </Button>
      ) : null}
      <Button ref={closeRef} onClick={onClose} className={closeButtonColumnClass(canSetDefault)}>
        Close
      </Button>
    </>
  )
}

function SelectedPathsList({ items }: { items: PropertiesDialogItem[] }) {
  return (
    <div className="grid gap-1">
      <div className={`text-caption ${TEXT_SECONDARY}`}>Selected paths</div>
      <ul
        className={`grid max-h-56 gap-0.5 overflow-auto font-mono text-caption ${SCROLLBAR_THIN}`}
      >
        {items.map((item) => (
          <li key={item.path} className="break-all">
            {item.path}
          </li>
        ))}
      </ul>
    </div>
  )
}

type DateDisplay = { dateFormat: DateFormat; showTime: boolean; showSeconds: boolean }

function yesOrNo(flag: boolean) {
  return flag ? 'Yes' : 'No'
}

function formatAttributes(attributes: string[]) {
  return attributes.length > 0 ? attributes.join(', ') : '—'
}

function SingleItemProperties({
  item,
  defaultAppRow,
  dateDisplay,
}: {
  item: PropertiesDialogItem
  defaultAppRow: ReactNode
  dateDisplay: DateDisplay
}) {
  return (
    <dl className="grid gap-1.5">
      <PropertyRow label="Type" value={item.typeLabel} />
      {defaultAppRow}
      <PropertyRow label="Path" value={item.path} monospace />
      <PropertyRow
        label="Size"
        value={formatItemSize(item.sizeBytes, item.itemCount, item.isDir)}
      />
      <PropertyRow label="Contains" value={formatContains(item.itemCount, item.isDir)} />
      <DatePropertyRow label="Modified" value={item.modifiedAt} {...dateDisplay} />
      <DatePropertyRow label="Created" value={item.createdAt} {...dateDisplay} />
      <PropertyRow label="Attributes" value={formatAttributes(item.attributes)} />
      <PropertyRow label="Hidden" value={yesOrNo(item.isHidden)} />
      <PropertyRow label="System" value={yesOrNo(item.isSystem)} />
    </dl>
  )
}

const LONE_BUTTON_COLUMN_CLASS = 'col-start-2'

function closeButtonColumnClass(canSetDefault: boolean) {
  return canSetDefault ? NO_CLASS : LONE_BUTTON_COLUMN_CLASS
}

const MONOSPACE_VALUE_CLASS = 'break-all font-mono text-caption'
const PROPORTIONAL_VALUE_CLASS = 'text-row'

function propertyValueFontClass(monospace: boolean) {
  return monospace ? MONOSPACE_VALUE_CLASS : PROPORTIONAL_VALUE_CLASS
}

function PropertyRow({
  label,
  value,
  monospace = false,
  valueClassName,
}: {
  label: string
  value: string
  monospace?: boolean
  valueClassName?: string
}) {
  return (
    <div className="flex items-baseline gap-4">
      <dt className={`w-24 shrink-0 text-caption ${TEXT_SECONDARY}`}>{label}</dt>
      <dd
        className={`min-w-0 flex-1 break-words ${valueClassName ?? ''} ${propertyValueFontClass(monospace)}`}
      >
        {value}
      </dd>
    </div>
  )
}

function defaultAppLabel({ loaded, app }: DefaultAppState): ReactNode {
  if (!loaded) {
    return 'Loading…'
  }
  if (!app) {
    return 'Not set'
  }
  return (
    <>
      <EntryIcon
        entry={{ name: app.name, isDir: false, iconDataUrl: app.iconDataUrl }}
        className="h-4 w-4 shrink-0"
      />
      <span className="truncate">{app.name}</span>
    </>
  )
}

function DefaultAppRow({ defaultApp }: { defaultApp: DefaultAppState }) {
  return (
    <div className="flex items-baseline gap-4">
      <dt className={`w-24 shrink-0 text-caption ${TEXT_SECONDARY}`}>Default App</dt>
      <dd className="flex min-w-0 flex-1 items-center gap-2 text-row">
        {defaultAppLabel(defaultApp)}
      </dd>
    </div>
  )
}

function DatePropertyRow({
  label,
  value,
  dateFormat,
  showTime,
  showSeconds,
}: {
  label: string
  value: string | null
  dateFormat: DateFormat
  showTime: boolean
  showSeconds: boolean
}) {
  const formatted = formatEntryDate(value, {
    format: dateFormat,
    showTime,
    showSeconds,
    relative: false,
  })
  // The default (un-coloured) tone keeps the dialog's standard value styling.
  const valueClassName =
    formatted.tone === 'default' ? undefined : dateToneClassName[formatted.tone]
  return <PropertyRow label={label} value={formatted.text} valueClassName={valueClassName} />
}
