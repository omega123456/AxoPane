import { saveSession } from '@/lib/ipc/commands'
import type { SessionPane, SessionState } from '@/lib/types/ipc'
import { activeTab, toSessionPane } from '@/stores/tabs-store'
import { useRecentFoldersStore } from '@/stores/recent-folders-store'
import { DEFAULT_TAB_VIEW } from '@/stores/panes/listing-transitions'
import type { PaneId } from '@/types/pane'
import { DEFAULT_SESSION_PATH } from '@/stores/panes/pane-state'

const SESSION_PERSIST_DELAY_MILLISECONDS = 200

let sessionPersistTimer: number | undefined

function buildSessionState(activePaneId: PaneId): SessionState {
  return {
    activePane: activePaneId,
    leftPath: activeTab('left').path,
    rightPath: activeTab('right').path,
    left: toSessionPane('left'),
    right: toSessionPane('right'),
    recentFolders: useRecentFoldersStore.getState().folders,
  }
}

export function schedulePersistSession(activePaneId: PaneId) {
  if (typeof window === 'undefined') {
    return
  }

  if (sessionPersistTimer) {
    window.clearTimeout(sessionPersistTimer)
  }

  sessionPersistTimer = window.setTimeout(() => {
    void saveSession(buildSessionState(activePaneId))
  }, SESSION_PERSIST_DELAY_MILLISECONDS)
}

export function sessionPaneOrDefault(
  pane: SessionPane | null | undefined,
  path: string,
): SessionPane {
  if (pane) {
    return pane
  }
  return {
    activeTabIndex: 0,
    tabs: [{ path: path || DEFAULT_SESSION_PATH, ...DEFAULT_TAB_VIEW, locked: false }],
  }
}
