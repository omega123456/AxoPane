import { AlertTriangleIcon } from '@/components/icons'
import { StatePanel } from './StatePanel'

type ErrorStateProps = {
  path: string
  message: string
  onRetry: () => void
}

export function ErrorState({ path, message, onRetry }: ErrorStateProps) {
  return (
    <StatePanel
      role="alert"
      danger
      icon={<AlertTriangleIcon className="size-6" />}
      title={`Couldn't open ${path}`}
      message={message}
      action={{ label: 'Try again', onClick: onRetry }}
    />
  )
}
