import { describe, expect, it } from 'vitest'
import type { ListingSession } from '@/stores/panes/listing-session'
import { throwIfSessionFailed } from '@/stores/panes/listing-runtime'

const FALLBACK_MESSAGE = 'Fallback failure'

function sessionWith(status: ListingSession['currentStatus'], error: string | null) {
  return { currentStatus: status, currentError: error } as ListingSession
}

describe('throwIfSessionFailed', () => {
  it('throws the session error, or the fallback when it has none', () => {
    expect(() => throwIfSessionFailed(sessionWith('error', 'Disk gone'), FALLBACK_MESSAGE)).toThrow(
      'Disk gone',
    )
    expect(() => throwIfSessionFailed(sessionWith('error', null), FALLBACK_MESSAGE)).toThrow(
      FALLBACK_MESSAGE,
    )
  })

  it('returns quietly while the session has not failed', () => {
    expect(() => throwIfSessionFailed(sessionWith('ready', null), FALLBACK_MESSAGE)).not.toThrow()
  })
})
