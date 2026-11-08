import { useMemo } from 'react'
import type { OpProgress } from '@/lib/types/ipc'
import { useQueueStore } from '@/stores/queue-store'

export function useOrderedOperations(): OpProgress[] {
  const order = useQueueStore((state) => state.order)
  const operationsMap = useQueueStore((state) => state.operations)
  return useMemo(
    () =>
      order
        .map((id) => operationsMap[id])
        .filter((operation): operation is OpProgress => operation !== undefined),
    [order, operationsMap],
  )
}
