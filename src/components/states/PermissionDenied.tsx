import { LockIcon } from '@/components/icons'
import { log } from '@/lib/app-log-commands'
import { openFullDiskAccessSettings, restartAsAdmin } from '@/lib/ipc/commands'
import { detectPlatformOs } from '@/lib/keymap'
import { StatePanel } from './StatePanel'

/** Offers the fix that matches the OS: elevation on Windows, Full Disk Access on macOS. */
export function PermissionDenied() {
  const isMac = detectPlatformOs() === 'macos'

  function fix() {
    const attempt = isMac ? openFullDiskAccessSettings() : restartAsAdmin()
    attempt.catch((cause: unknown) => {
      log.error('permission fix failed', {
        platform: isMac ? 'macos' : 'windows',
        error: String(cause),
      })
    })
  }

  return (
    <StatePanel
      role="alert"
      label="Permission denied"
      icon={<LockIcon className="size-6" />}
      title="You don't have access"
      message={
        isMac
          ? 'macOS needs Full Disk Access for AxoPane.'
          : 'Windows blocks this folder for your account.'
      }
      action={{
        label: isMac ? 'Open System Settings' : 'Open as administrator',
        onClick: fix,
      }}
    />
  )
}
