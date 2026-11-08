export type RejectionWithKind = { kind: unknown }

export function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause)
}

export function isRejectionWithKind(cause: unknown): cause is RejectionWithKind {
  if (typeof cause !== 'object') {
    return false
  }
  if (cause === null) {
    return false
  }
  return 'kind' in cause
}
