import { requestIcons } from '@/lib/ipc/commands'
import type { IconStateEvent } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
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
    usePanesStore.getState().panes[paneId].entries,
    paths,
    usePanesStore.getState(),
  )
  if (pending.length === 0) {
    return
  }

  usePanesStore.setState((state) => withPendingIconRequests(state, pending, true))
  try {
    await requestIcons({ paths: pending })
  } catch (error) {
    usePanesStore.setState((state) => withPendingIconRequests(state, pending, false))
    throw error
  }
}

export function applyIconStates(events: IconStateEvent[]) {
  const input = { events, now: cacheNow(), windowsPlatform: isWindowsNavigator() }
  usePanesStore.setState((state) => appliedIconStates(state, input))
}
