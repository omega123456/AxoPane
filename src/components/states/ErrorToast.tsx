import { InfoBar } from '@/components/shell/InfoBar'
import { useErrorToastStore } from '@/stores/error-toast-store'

export function ErrorToast() {
  const message = useErrorToastStore((state) => state.message)
  const dismiss = useErrorToastStore((state) => state.dismiss)

  if (!message) {
    return null
  }

  return (
    <div className="pointer-events-none absolute inset-x-0 top-2 z-30 flex justify-center px-3">
      <InfoBar
        severity="error"
        onDismiss={dismiss}
        dismissLabel="Dismiss error"
        className="pointer-events-auto w-infobar max-w-full shadow-light-flyout dark:shadow-dark-flyout"
      >
        {message}
      </InfoBar>
    </div>
  )
}
