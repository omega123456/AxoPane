import { useEffect, useState } from 'react'
import { log } from '@/lib/app-log-commands'
import { getAppVersion } from '@/lib/updater'

const PENDING_VERSION = '…'
const UNKNOWN_VERSION = 'unknown'

export function useAppVersion() {
  const [appVersion, setAppVersion] = useState(PENDING_VERSION)

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
          setAppVersion(UNKNOWN_VERSION)
        }
      })
    return () => {
      cancelled = true
    }
  }, [])

  return appVersion
}
