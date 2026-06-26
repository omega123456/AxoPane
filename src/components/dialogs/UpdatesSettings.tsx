import { useEffect, useState } from 'react'
import { AlertTriangleIcon, CheckIcon, DownloadIcon, RefreshIcon } from '@/components/icons'
import { Button, SectionLabel, SelectField, SettingRow } from '@/components/controls'
import { log } from '@/lib/app-log-commands'
import { getAppVersion } from '@/lib/updater'
import { UPDATE_INTERVAL_OPTIONS, type UpdateInterval } from '@/lib/update-intervals'
import { useUpdaterStore } from '@/stores/updater-store'

export function UpdatesSettings({
  value,
  onChange,
}: {
  value: UpdateInterval
  onChange: (value: UpdateInterval) => void
}) {
  const status = useUpdaterStore((state) => state.status)
  const summary = useUpdaterStore((state) => state.summary)
  const error = useUpdaterStore((state) => state.error)
  const checkForUpdate = useUpdaterStore((state) => state.checkForUpdate)
  const downloadAndInstall = useUpdaterStore((state) => state.downloadAndInstall)

  const [appVersion, setAppVersion] = useState('…')

  useEffect(() => {
    let cancelled = false
    void getAppVersion()
      .then((version) => {
        if (!cancelled) {
          setAppVersion(version)
        }
      })
      .catch((cause: unknown) => {
        log.error('failed to read app version', { error: String(cause) })
        if (!cancelled) {
          setAppVersion('unknown')
        }
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <div data-testid="settings-updates">
      <SectionLabel className="mb-3">Software Updates</SectionLabel>

      <SettingRow
        fixedCopy
        title="Current version"
        description="The version of AxoPane you are running"
        control={
          <span
            data-testid="updates-app-version"
            className="w-50 text-right font-mono text-row text-light-text dark:text-dark-text"
          >
            {appVersion}
          </span>
        }
      />

      <div className="border-b border-light-border py-4 dark:border-dark-border">
        <UpdateStatus
          status={status}
          version={summary?.version ?? null}
          error={error}
          appVersion={appVersion}
          onCheck={() => void checkForUpdate(true)}
          onInstall={() => void downloadAndInstall()}
        />
      </div>

      <SectionLabel className="mb-3 mt-5">Automatic Updates</SectionLabel>
      <SettingRow
        fixedCopy
        title="Check for updates"
        description="How often AxoPane looks for a newer version in the background"
        control={
          <SelectField
            ariaLabel="Update check frequency"
            value={value}
            onChange={onChange}
            options={UPDATE_INTERVAL_OPTIONS.map(({ value: optionValue, label }) => ({
              value: optionValue,
              label,
            }))}
          />
        }
      />
    </div>
  )
}

function UpdateStatus({
  status,
  version,
  error,
  appVersion,
  onCheck,
  onInstall,
}: {
  status: ReturnType<typeof useUpdaterStore.getState>['status']
  version: string | null
  error: string | null
  appVersion: string
  onCheck: () => void
  onInstall: () => void
}) {
  const checkButton = (
    <Button onClick={onCheck} data-testid="updates-check-button">
      <span className="inline-flex items-center gap-2">
        <RefreshIcon className="size-4" />
        Check for updates
      </span>
    </Button>
  )

  switch (status) {
    case 'checking':
      return (
        <Button disabled data-testid="updates-checking">
          <span className="inline-flex items-center gap-2">
            <RefreshIcon className="size-4 animate-spin" />
            Checking…
          </span>
        </Button>
      )
    case 'up-to-date':
      return (
        <div className="flex items-center gap-3" data-testid="updates-up-to-date">
          <span className="inline-flex items-center gap-2 text-row text-accent-green">
            <CheckIcon className="size-4" />
            You&apos;re up to date (v{appVersion})
          </span>
          {checkButton}
        </div>
      )
    case 'available':
      return (
        <div
          className="flex items-center justify-between gap-3 rounded-tab border border-accent-blue-border bg-accent-blue-soft px-3 py-2.5"
          data-testid="updates-available"
        >
          <span className="text-row text-light-text dark:text-dark-text">
            Version {version ?? 'new'} is available
          </span>
          <Button variant="primary" onClick={onInstall} data-testid="updates-install-button">
            <span className="inline-flex items-center gap-2">
              <DownloadIcon className="size-4" />
              Download &amp; install
            </span>
          </Button>
        </div>
      )
    case 'installing':
      return (
        <Button disabled data-testid="updates-installing">
          <span className="inline-flex items-center gap-2">
            <DownloadIcon className="size-4 animate-spin" />
            Installing…
          </span>
        </Button>
      )
    case 'error':
      return (
        <div className="flex items-center gap-3" data-testid="updates-error">
          <span className="inline-flex items-center gap-2 text-row text-accent-amber">
            <AlertTriangleIcon className="size-4" />
            {error ?? 'Update check failed'}
          </span>
          {checkButton}
        </div>
      )
    case 'idle':
    default:
      return checkButton
  }
}
