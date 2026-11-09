import { Button } from '@/components/controls/Button'
import { InfoBar } from '@/components/shell/InfoBar'
import { useUpdaterStore } from '@/stores/updater-store'

export function UpdateBanner() {
  const { summary, update, status, error } = useUpdaterStore((state) => state.outcome)
  const downloadAndInstall = useUpdaterStore((state) => state.downloadAndInstall)
  const dismiss = useUpdaterStore((state) => state.dismiss)

  if (!summary) {
    return null
  }
  if (!update) {
    return null
  }

  const installing = status === 'installing'
  const failed = status === 'error'

  return (
    <InfoBar
      severity={failed ? 'error' : 'info'}
      label="Update available"
      title={failed ? 'Update failed' : 'Update ready'}
      action={
        <Button disabled={installing} onClick={() => void downloadAndInstall()}>
          {installing ? 'Installing…' : 'Restart'}
        </Button>
      }
      onDismiss={dismiss}
      dismissLabel="Dismiss update banner"
      dismissDisabled={installing}
    >
      {failed ? error : `AxoPane ${summary.version} installs when you restart.`}
    </InfoBar>
  )
}
