import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
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
import { SCROLLBAR_THIN } from '@/lib/class-names'
import { TEXT_SECONDARY } from '@/lib/class-names'

function formatItemSize(sizeBytes: number | null, itemCount: number | null, isDir: boolean) {
  if (typeof sizeBytes === 'number') {
    return formatBytes(sizeBytes)
  }

  if (isDir && typeof itemCount === 'number') {
    return `${formatCount(itemCount)} ${itemCount === 1 ? 'item' : 'items'}`
  }

  return '—'
}

function formatContains(itemCount: number | null, isDir: boolean) {
  if (!isDir || typeof itemCount !== 'number') {
    return '—'
  }

  return `${formatCount(itemCount)} ${itemCount === 1 ? 'item' : 'items'}`
}

export function PropertiesDialog() {
  const dialog = usePropertiesDialogStore((state) => state.dialog)
  const close = usePropertiesDialogStore((state) => state.close)
  const dateFormat = useConfigStore((state) => state.dateFormat)
  const showTime = useConfigStore((state) => state.showTime)
  const showSeconds = useConfigStore((state) => state.showSeconds)
  const closeRef = useRef<HTMLButtonElement>(null)
  const defaultAppPickerGuardRef = useRef<{ cancelled: boolean } | null>(null)
  const defaultAppDialogClosed = useDefaultAppDialogStore((state) => state.dialog === null)
  const [defaultApp, setDefaultApp] = useState<MacApp | null>(null)
  const [defaultAppLoaded, setDefaultAppLoaded] = useState(false)
  const [isLoadingDefaultAppPicker, setIsLoadingDefaultAppPicker] = useState(false)
  const [prevDialogForDefaultApp, setPrevDialogForDefaultApp] =
    useState<PropertiesDialogState | null>(dialog)

  // Reset render-derived state synchronously when Properties re-opens for a
  // different selection, rather than in an effect (avoids a cascading-render
  // setState-in-effect, since this only fires when `dialog` identity changes).
  if (dialog !== prevDialogForDefaultApp) {
    setPrevDialogForDefaultApp(dialog)
    setDefaultApp(null)
    setDefaultAppLoaded(false)
    setIsLoadingDefaultAppPicker(false)
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
    if (!dialog || !canSetDefaultApplication(dialog.items)) {
      return
    }

    let cancelled = false
    void getDefaultApplication(dialog.items[0].path).then((app) => {
      if (cancelled) {
        return
      }
      setDefaultApp(app)
      setDefaultAppLoaded(true)
    })

    return () => {
      cancelled = true
    }
    // Re-run when the Set Default Application picker closes so a freshly
    // chosen app is reflected here without requiring the user to reopen
    // Properties.
  }, [dialog, defaultAppDialogClosed])

  const summary = useMemo(() => {
    if (!dialog) {
      return null
    }

    const singleItem = dialog.items.length === 1 ? dialog.items[0] : null
    return {
      heading: singleItem ? singleItem.name : `${formatCount(dialog.items.length)} items selected`,
      itemCountLabel: `${formatCount(dialog.items.length)} ${dialog.items.length === 1 ? 'item' : 'items'}`,
      singleItem,
    }
  }, [dialog])

  if (!dialog || !summary) {
    return null
  }

  const singleItem = summary.singleItem

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape' || event.key === 'Enter') {
      event.preventDefault()
      close()
    }
  }

  async function openDefaultAppDialog() {
    if (!singleItem) {
      return
    }

    const filePath = singleItem.path
    const fileName = singleItem.name
    const guard = { cancelled: false }
    defaultAppPickerGuardRef.current = guard

    setIsLoadingDefaultAppPicker(true)
    const apps = await listApplications()

    if (guard.cancelled) {
      return
    }

    setIsLoadingDefaultAppPicker(false)
    useDefaultAppDialogStore.getState().open({ filePath, fileName, apps })
  }

  const canSetDefault = canSetDefaultApplication(dialog.items)
  return (
    <DialogShell
      label="Properties"
      title={summary.heading}
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <>
          {canSetDefault ? (
            <Button
              disabled={isLoadingDefaultAppPicker}
              onClick={() => void openDefaultAppDialog()}
            >
              {isLoadingDefaultAppPicker ? (
                <>
                  <LoaderCircleIcon className="size-4 animate-spin" />
                  Loading…
                </>
              ) : (
                'Set Default Application…'
              )}
            </Button>
          ) : null}
          <Button ref={closeRef} onClick={close} className={canSetDefault ? '' : 'col-start-2'}>
            Close
          </Button>
        </>
      }
    >
      <p className={`-mt-2 text-caption ${TEXT_SECONDARY}`}>{summary.itemCountLabel}</p>
      {summary.singleItem ? (
        <SingleItemProperties
          item={summary.singleItem}
          defaultAppRow={
            canSetDefault ? <DefaultAppRow loaded={defaultAppLoaded} app={defaultApp} /> : null
          }
          dateDisplay={{ dateFormat, showTime, showSeconds }}
        />
      ) : (
        <div className="grid gap-1">
          <div className={`text-caption ${TEXT_SECONDARY}`}>Selected paths</div>
          <ul
            className={`grid max-h-56 gap-0.5 overflow-auto font-mono text-caption ${SCROLLBAR_THIN}`}
          >
            {dialog.items.map((item) => (
              <li key={item.path} className="break-all">
                {item.path}
              </li>
            ))}
          </ul>
        </div>
      )}
    </DialogShell>
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
        className={`min-w-0 flex-1 break-words ${valueClassName ?? ''} ${
          monospace ? 'break-all font-mono text-caption' : 'text-row'
        }`}
      >
        {value}
      </dd>
    </div>
  )
}

function DefaultAppRow({ loaded, app }: { loaded: boolean; app: MacApp | null }) {
  return (
    <div className="flex items-baseline gap-4">
      <dt className={`w-24 shrink-0 text-caption ${TEXT_SECONDARY}`}>Default App</dt>
      <dd className="flex min-w-0 flex-1 items-center gap-2 text-row">
        {!loaded ? (
          'Loading…'
        ) : app ? (
          <>
            <EntryIcon
              entry={{ name: app.name, isDir: false, iconDataUrl: app.iconDataUrl }}
              className="h-4 w-4 shrink-0"
            />
            <span className="truncate">{app.name}</span>
          </>
        ) : (
          'Not set'
        )}
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
