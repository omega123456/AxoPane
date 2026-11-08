import { requestIcons } from '@/lib/ipc/commands'
import { isWindowsPlatform } from '@/lib/keymap'
import type { IconStateEvent } from '@/lib/types/ipc'
import { panesStore } from '@/stores/panes/store-handle'
import {
  appliedIconStates,
  collectPendingIconRequests,
  withPendingIconRequests,
} from '@/stores/panes/icon-requests'
import { cacheNow } from '@/stores/weighted-cache'
import type { PaneId } from '@/types/pane'

export async function requestVisibleIcons(paneId: PaneId, paths: string[]) {
  const pending = collectPendingIconRequests(
    panesStore().getState().panes[paneId].entries,
    paths,
    panesStore().getState(),
  )
  if (pending.length === 0) {
    return
  }

  panesStore().setState((state) => withPendingIconRequests(state, pending, true))
  try {
    await requestIcons({ paths: pending })
  } catch (error) {
    panesStore().setState((state) => withPendingIconRequests(state, pending, false))
    throw error
  }
}

export function applyIconStates(events: IconStateEvent[]) {
  const windowsPlatform = isWindowsPlatform()
  const input = { events, now: cacheNow(), windowsPlatform }
  panesStore().setState((state) => appliedIconStates(state, input))
}
