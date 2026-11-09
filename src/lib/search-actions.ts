import { log } from '@/lib/app-log-commands'
import { openPath } from '@/lib/ipc/commands'
import { resolveCommandForEvent } from '@/lib/keymap'
import type { CommandId, SearchHit, Shortcut } from '@/lib/types/ipc'
import { getParentPath, usePanesStore } from '@/stores/panes-store'
import { otherPane } from '@/lib/pane-id'
import { hasPrimaryModifier, ENTER_KEY } from '@/lib/key-table'
import { POSIX_SEPARATOR, WINDOWS_SEPARATOR } from '@/lib/path-compare'

export type SearchActivation = 'default' | 'open' | 'newTab' | 'otherPane'

export async function activateSearchHit(hit: SearchHit, how: SearchActivation) {
  const panes = usePanesStore.getState()
  const paneId = panes.activePaneId
  const launchesFile = !hit.isDir && how === 'open'
  if (launchesFile) {
    await openPath({ path: hit.path }).catch((error: unknown) => {
      log.error('search open failed', { path: hit.path, error })
    })
    return
  }
  const folder = hit.isDir ? hit.path : (getParentPath(hit.path) ?? hit.path)
  const revealPath = hit.isDir ? undefined : hit.path
  const targetPaneId = how === 'otherPane' ? otherPane(paneId) : paneId
  await panes.openTabFromPath(targetPaneId, folder, { revealPath })
}

const activationByCommand: Partial<Record<CommandId, SearchActivation>> = {
  open: 'default',
  openInNewTab: 'newTab',
  openInOtherPane: 'otherPane',
}

function isPlainShiftEnter(event: KeyboardEvent) {
  return event.key === ENTER_KEY && event.shiftKey && !hasPrimaryModifier(event)
}

export function searchActivationForEvent(
  event: KeyboardEvent,
  keymap: Record<CommandId, Shortcut[]>,
): SearchActivation | null {
  if (isPlainShiftEnter(event)) {
    return 'open'
  }
  const commandId = resolveCommandForEvent(event, keymap)
  return commandId ? (activationByCommand[commandId] ?? null) : null
}

export function completeSearchQuery(query: string, hit: SearchHit) {
  const cut = Math.max(query.lastIndexOf(POSIX_SEPARATOR), query.lastIndexOf(WINDOWS_SEPARATOR))
  const separator = cut >= 0 ? query[cut] : POSIX_SEPARATOR
  return `${query.slice(0, cut + 1)}${hit.name}${separator}`
}
