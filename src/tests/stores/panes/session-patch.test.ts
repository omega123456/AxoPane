import { describe, expect, it } from 'vitest'
import type { SessionBaseline, SessionPatchEvent } from '@/lib/types/ipc'
import {
  dirPatchFromSessionPatch,
  replacementViewApplied,
  sessionPaneId,
} from '@/stores/panes/session-patch'
import type { TreeNodeState } from '@/stores/panes/pane-state'
import { directoryEntry, fileEntry, panesDataWithLeft } from './pane-fixtures'

const BASELINE = {} as SessionBaseline
const TARGET = { tabId: 'left-tab-1', path: '/root' }

function deltaEvent(paneId: string): SessionPatchEvent {
  return {
    mode: 'delta',
    paneId,
    ...TARGET,
    previousBaseline: BASELINE,
    nextBaseline: BASELINE,
    totalRows: 2,
    deltas: [
      { kind: 'inserted', rowIndex: 0, entry: fileEntry('/root/new') },
      { kind: 'removed', rowIndex: 1, path: '/root/old' },
    ],
  }
}

describe('session patch', () => {
  it('accepts only real pane ids', () => {
    expect(sessionPaneId(deltaEvent('right'))).toBe('right')
    expect(sessionPaneId(deltaEvent('middle'))).toBeNull()
  })

  it('recasts delta, metadata and replacement patches as watcher patches', () => {
    expect(dirPatchFromSessionPatch(deltaEvent('left'))).toEqual({
      ...TARGET,
      reason: 'watch',
      changed: [{ path: '/root/new', entry: fileEntry('/root/new') }],
      removed: ['/root/old'],
    })
    const metadata: SessionPatchEvent = {
      mode: 'metadataOnly',
      paneId: 'left',
      ...TARGET,
      baseline: BASELINE,
      updates: [{ path: '/root/a', entry: fileEntry('/root/a') }],
    }
    expect(dirPatchFromSessionPatch(metadata).changed).toHaveLength(1)
    const replaceView: SessionPatchEvent = {
      mode: 'replaceView',
      paneId: 'left',
      ...TARGET,
      previousBaseline: BASELINE,
      nextBaseline: BASELINE,
      totalRows: 0,
    }
    expect(dirPatchFromSessionPatch(replaceView)).toMatchObject({ changed: [], removed: [] })
  })

  it('installs a replacement view and reconciles the tree folders', () => {
    const node: TreeNodeState = {
      id: '/root',
      name: 'root',
      path: '/root',
      parentPath: null,
      children: ['/root/gone'],
      expanded: true,
      loaded: true,
    }
    const state = panesDataWithLeft(
      {
        path: '/root',
        sortKey: 'size',
        sortDirection: 'desc',
        entries: [directoryEntry('/root/gone')],
        focusedEntryId: '/root/gone',
      },
      {
        treeNodes: { '/root': node },
        passiveItemCountRequests: { 'left::t::1::/x': true },
      },
    )
    const next = replacementViewApplied(state, {
      paneId: 'left',
      ...TARGET,
      requestId: 4,
      loadedEntries: [fileEntry('/root/small', { sizeBytes: 1 }), directoryEntry('/root/kept')],
    })
    expect(next.panes?.left.entries.map((entry) => entry.path)).toEqual([
      '/root/kept',
      '/root/small',
    ])
    expect(next.panes?.left.listRequestId).toBe(4)
    expect(next.panes?.left.focusedEntryId).toBe('/root/kept')
    expect(next.passiveItemCountRequests).toEqual({})
    expect(next.treeNodes?.['/root'].children).toEqual(['/root/kept'])
  })
})
