import { requestIcons } from '@/lib/ipc/commands'
import type { IconStateEvent } from '@/lib/types/ipc'
import { panesStore } from '@/stores/panes/store-handle'
import {
  appliedIconStates,
  collectPendingIconRequests,
  withPendingIconRequests,
} from '@/stores/panes/icon-requests'
import { cacheNow } from '@/stores/weighted-cache'
import type { PaneId } from '@/types/pane'

const WINDOWS_PLATFORM_PREFIX = 'Win'

function isWindowsNavigator() {
  return typeof navigator !== 'undefined' && navigator.platform.startsWith(WINDOWS_PLATFORM_PREFIX)
}

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
  const input = { events, now: cacheNow(), windowsPlatform: isWindowsNavigator() }
  panesStore().setState((state) => appliedIconStates(state, input))
}
