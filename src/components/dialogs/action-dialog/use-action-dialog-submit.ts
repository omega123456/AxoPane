import { log } from '@/lib/app-log-commands'
import { errorMessage } from '@/lib/error-message'
import { useActionDialogStore } from '@/stores/action-dialog-store'

export type ActionFailureLog = {
  message: string
  context?: Record<string, unknown>
}

export function useActionDialogSubmit() {
  const close = useActionDialogStore((state) => state.close)
  const busy = useActionDialogStore((state) => state.busy)
  const error = useActionDialogStore((state) => state.error)
  const setBusy = useActionDialogStore((state) => state.setBusy)
  const setError = useActionDialogStore((state) => state.setError)

  async function run(action: () => Promise<unknown>, failure: ActionFailureLog) {
    if (busy) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      await action()
      close()
    } catch (cause) {
      const message = errorMessage(cause)
      log.error(failure.message, { ...failure.context, error: message })
      setError(message)
      setBusy(false)
    }
  }

  return { close, busy, error, run }
}
