export type PendingMarkedRequest = {
  markPending: (isPending: boolean) => void
  request: () => Promise<unknown>
}

export async function runMarkingPending({ markPending, request }: PendingMarkedRequest) {
  markPending(true)
  try {
    await request()
  } catch (error) {
    markPending(false)
    throw error
  }
}
