import { useEffect, useState } from 'react'
import {
  AlertTriangleIcon,
  CheckIcon,
  DownloadIcon,
  InfoIcon,
  RefreshIcon,
} from '@/components/icons'
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
    <div data-testid="settings-updates" className="contents">
      <SectionLabel>Software updates</SectionLabel>

      <SettingRow
        icon={<InfoIcon className="size-4" />}
        title="Current version"
        description="The version of AxoPane you are running"
        control={
          <span
            data-testid="updates-app-version"
            className="font-mono text-row text-light-text-secondary dark:text-dark-text-secondary"
          >
            {appVersion}
          </span>
        }
      />

      <div className="rounded-card border border-light-stroke bg-light-card px-4 py-3 dark:border-dark-stroke dark:bg-dark-card">
        <UpdateStatus
          status={status}
          version={summary?.version ?? null}
          error={error}
          appVersion={appVersion}
          onCheck={() => void checkForUpdate(true)}
          onInstall={() => void downloadAndInstall()}
        />
      </div>

      <SectionLabel>Automatic updates</SectionLabel>
      <SettingRow
        icon={<RefreshIcon className="size-4" />}
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
          <span className="inline-flex flex-1 items-center gap-2 text-body text-light-success dark:text-dark-success">
            <CheckIcon className="size-4" />
            You&apos;re up to date (v{appVersion})
          </span>
          {checkButton}
        </div>
      )
    case 'available':
      return (
        <div className="flex items-center justify-between gap-3" data-testid="updates-available">
          <span className="text-body text-light-text dark:text-dark-text">
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
          <span className="inline-flex flex-1 items-center gap-2 text-body text-light-warning dark:text-dark-warning">
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
