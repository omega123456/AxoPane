import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import { DialogShell } from '@/components/dialogs/DialogShell'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { canSetDefaultApplication, getDefaultApplication } from '@/lib/app-picker-commands'
import { dateToneClassName, type DateFormat, formatEntryDate } from '@/lib/date-format'
import { formatBytes, formatCount } from '@/lib/format'
import { useConfigStore } from '@/stores/config-store'
import { useDefaultAppDialogStore } from '@/stores/default-app-dialog-store'
import {
  usePropertiesDialogStore,
  type PropertiesDialogState,
} from '@/stores/properties-dialog-store'
import type { MacApp } from '@/lib/types/ipc'

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
  const defaultAppDialogClosed = useDefaultAppDialogStore((state) => state.dialog === null)
  const [defaultApp, setDefaultApp] = useState<MacApp | null>(null)
  const [defaultAppLoaded, setDefaultAppLoaded] = useState(false)
  const [prevDialogForDefaultApp, setPrevDialogForDefaultApp] =
    useState<PropertiesDialogState | null>(dialog)

  // Reset render-derived state synchronously when Properties re-opens for a
  // different selection, rather than in an effect (avoids a cascading-render
  // setState-in-effect, since this only fires when `dialog` identity changes).
  if (dialog !== prevDialogForDefaultApp) {
    setPrevDialogForDefaultApp(dialog)
    setDefaultApp(null)
    setDefaultAppLoaded(false)
  }

  useEffect(() => {
    if (!dialog) {
      return
    }

    closeRef.current?.focus()
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

  function openDefaultAppDialog() {
    if (!singleItem) {
      return
    }
    useDefaultAppDialogStore.getState().open({
      filePath: singleItem.path,
      fileName: singleItem.name,
    })
  }

  return (
    <DialogShell label="Properties" onDismiss={close} onKeyDown={onKeyDown}>
      <div className="border-b border-light-border px-4 py-3 dark:border-dark-border">
        <div className="text-sm font-semibold text-light-text dark:text-dark-text">Properties</div>
      </div>
      <div className="space-y-4 p-4">
        <div>
          <div className="text-sm font-semibold text-light-text dark:text-dark-text">
            {summary.heading}
          </div>
          <div className="mt-1 text-row text-light-text-muted dark:text-dark-text-muted">
            {summary.itemCountLabel}
          </div>
        </div>

        {summary.singleItem ? (
          <dl className="space-y-2 text-row">
            <PropertyRow label="Type" value={summary.singleItem.typeLabel} />
            {canSetDefaultApplication(dialog.items) ? (
              <DefaultAppRow loaded={defaultAppLoaded} app={defaultApp} />
            ) : null}
            <PropertyRow label="Path" value={summary.singleItem.path} monospace />
            <PropertyRow
              label="Size"
              value={formatItemSize(
                summary.singleItem.sizeBytes,
                summary.singleItem.itemCount,
                summary.singleItem.isDir,
              )}
            />
            <PropertyRow
              label="Contains"
              value={formatContains(summary.singleItem.itemCount, summary.singleItem.isDir)}
            />
            <DatePropertyRow
              label="Modified"
              value={summary.singleItem.modifiedAt}
              dateFormat={dateFormat}
              showTime={showTime}
              showSeconds={showSeconds}
            />
            <DatePropertyRow
              label="Created"
              value={summary.singleItem.createdAt}
              dateFormat={dateFormat}
              showTime={showTime}
              showSeconds={showSeconds}
            />
            <PropertyRow
              label="Attributes"
              value={
                summary.singleItem.attributes.length > 0
                  ? summary.singleItem.attributes.join(', ')
                  : '—'
              }
            />
            <PropertyRow label="Hidden" value={summary.singleItem.isHidden ? 'Yes' : 'No'} />
            <PropertyRow label="System" value={summary.singleItem.isSystem ? 'Yes' : 'No'} />
          </dl>
        ) : (
          <div>
            <div className="text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
              Selected paths
            </div>
            <ul className="mt-2 max-h-56 space-y-1 overflow-auto rounded-tab border border-light-border bg-light-window p-3 font-mono text-uxs text-light-text scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:border-dark-border dark:bg-dark-window dark:text-dark-text dark:scrollbar-thumb-dark-text-faint">
              {dialog.items.map((item) => (
                <li key={item.path} className="break-all">
                  {item.path}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
      <div className="flex justify-end gap-2 border-t border-light-border p-4 dark:border-dark-border">
        {canSetDefaultApplication(dialog.items) ? (
          <button
            type="button"
            onClick={openDefaultAppDialog}
            className="rounded-md border border-light-border px-4 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
          >
            Set Default Application…
          </button>
        ) : null}
        <button
          ref={closeRef}
          type="button"
          onClick={close}
          className="rounded-md border border-light-border px-4 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
        >
          Close
        </button>
      </div>
    </DialogShell>
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
    <div className="flex justify-between gap-4 border-b border-light-border py-2 dark:border-dark-border">
      <dt className="shrink-0 text-light-text-muted dark:text-dark-text-muted">{label}</dt>
      <dd
        className={`text-right ${valueClassName ?? 'text-light-text-soft dark:text-dark-text-soft'} ${
          monospace ? 'break-all font-mono text-uxs' : ''
        }`}
      >
        {value}
      </dd>
    </div>
  )
}

function DefaultAppRow({ loaded, app }: { loaded: boolean; app: MacApp | null }) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-light-border py-2 dark:border-dark-border">
      <dt className="shrink-0 text-light-text-muted dark:text-dark-text-muted">Default App</dt>
      <dd className="flex min-w-0 items-center justify-end gap-2 text-right text-light-text-soft dark:text-dark-text-soft">
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
