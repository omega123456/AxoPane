import { log } from '@/lib/app-log-commands'
import { openPath } from '@/lib/ipc/commands'
import { resolveCommandForEvent } from '@/lib/keymap'
import type { CommandId, SearchHit, Shortcut } from '@/lib/types/ipc'
import { getParentPath, usePanesStore } from '@/stores/panes-store'

/** `open` launches a file; folders always open in a new tab in a pane. */
export type SearchActivation = 'default' | 'open' | 'newTab' | 'otherPane'

/**
 * Acts on a Search everywhere hit: a folder opens in a new tab of the active
 * pane, a file is revealed (selected in its folder) unless `open` asks to
 * launch it.
 */
export async function activateSearchHit(hit: SearchHit, how: SearchActivation) {
  const panes = usePanesStore.getState()
  const paneId = panes.activePaneId
  if (!hit.isDir && how === 'open') {
    await openPath({ path: hit.path }).catch((error: unknown) => {
      log.error('search open failed', { path: hit.path, error })
    })
    return
  }
  const folder = hit.isDir ? hit.path : (getParentPath(hit.path) ?? hit.path)
  const revealPath = hit.isDir ? undefined : hit.path
  if (how === 'otherPane') {
    await panes.openTabFromPath(paneId === 'left' ? 'right' : 'left', folder, { revealPath })
  } else if (how === 'newTab' || hit.isDir) {
    await panes.openTabFromPath(paneId, folder, { revealPath })
  } else {
    await panes.navigatePane(paneId, folder, { revealPath })
  }
}

/**
 * Enter opens or reveals, Shift+Enter launches a file, and the remappable
 * "Open in new tab" / "Open in other pane" bindings choose those targets.
 */
export function searchActivationForEvent(
  event: KeyboardEvent,
  keymap: Record<CommandId, Shortcut[]>,
): SearchActivation | null {
  if (event.key === 'Enter' && event.shiftKey && !event.ctrlKey && !event.metaKey) {
    return 'open'
  }
  switch (resolveCommandForEvent(event, keymap)) {
    case 'open':
      return 'default'
    case 'openInNewTab':
      return 'newTab'
    case 'openInOtherPane':
      return 'otherPane'
    default:
      return null
  }
}

/** Tab completion: the folder's name replaces the last segment, plus a separator to list its children. */
export function completeSearchQuery(query: string, hit: SearchHit) {
  const cut = Math.max(query.lastIndexOf('/'), query.lastIndexOf('\\'))
  const separator = cut >= 0 ? query[cut] : '/'
  return `${query.slice(0, cut + 1)}${hit.name}${separator}`
}
