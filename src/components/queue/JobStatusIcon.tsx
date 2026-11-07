import type { ReactNode } from 'react'
import { CheckCircleIcon, XCircleIcon } from '@/components/icons'
import type { OpStatus } from '@/lib/types/ipc'
import { TEXT_SECONDARY } from '@/lib/class-names'

const statusIcons: Partial<Record<OpStatus, ReactNode>> = {
  completed: (
    <CheckCircleIcon className="size-4 shrink-0 text-light-success dark:text-dark-success" />
  ),
  failed: <XCircleIcon className="size-4 shrink-0 text-light-danger dark:text-dark-danger" />,
  cancelled: <XCircleIcon className={`size-4 shrink-0 ${TEXT_SECONDARY}`} />,
}

export function JobStatusIcon({ status }: { status: OpStatus }) {
  return statusIcons[status] ?? null
}
