import { useEffect, useMemo, useRef, type KeyboardEvent } from 'react'
import { DialogShell } from '@/components/dialogs/DialogShell'
import { dateToneClassName, type DateFormat, formatEntryDate } from '@/lib/date-format'
import { formatBytes, formatCount } from '@/lib/format'
import { useConfigStore } from '@/stores/config-store'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'

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

  useEffect(() => {
    if (!dialog) {
      return
    }

    closeRef.current?.focus()
  }, [dialog])

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

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape' || event.key === 'Enter') {
      event.preventDefault()
      close()
    }
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
      <div className="flex justify-end border-t border-light-border p-4 dark:border-dark-border">
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
