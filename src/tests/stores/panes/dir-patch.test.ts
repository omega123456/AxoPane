import { describe, expect, it } from 'vitest'
import type { DirPatchEvent } from '@/lib/types/ipc'
import { dirPatchOutcome, paneIdFromTabId, patchEntries } from '@/stores/panes/dir-patch'
import type { TreeNodeState } from '@/stores/panes/pane-state'
import { directoryEntry, fileEntry, panesDataWithLeft, paneWith } from './pane-fixtures'

const TAB_ID = 'left-tab-1'

function patch(changes: Partial<DirPatchEvent> = {}): DirPatchEvent {
  return { tabId: TAB_ID, path: '/root', reason: 'watch', changed: [], removed: [], ...changes }
}

function loadedNode(path: string, children: string[]): TreeNodeState {
  return {
    id: path,
    name: path,
    path,
    parentPath: null,
    children,
    expanded: true,
    loaded: true,
  }
}

const PATCH_STATE = { sizeStates: {}, showHiddenFiles: false, resolvedIconPaths: {} }

describe('dir patch', () => {
  it('maps tab ids to panes', () => {
    expect(paneIdFromTabId('right-tab-3')).toBe('right')
    expect(paneIdFromTabId('left-tab-1')).toBe('left')
  })

  it('removes, replaces and inserts rows in sort order, dropping filtered-out changes', () => {
    const pane = paneWith('left', {
      path: '/root',
      filterApplied: 'a',
      entries: [fileEntry('/root/a1'), fileEntry('/root/a3'), fileEntry('/root/a4')],
    })
    const event = patch({
      removed: ['/root/a4'],
      changed: [
        { path: '/root/a2', entry: fileEntry('/root/a2') },
        { path: '/root/a2', entry: fileEntry('/root/a2', { sizeBytes: 5 }) },
        { path: '/root/b', entry: fileEntry('/root/b') },
        { path: '/root/hidden-a', entry: fileEntry('/root/hidden-a', { isHidden: true }) },
      ],
    })
    const entries = patchEntries(pane, event, PATCH_STATE)
    expect(entries?.map((entry) => entry.path)).toEqual(['/root/a1', '/root/a2', '/root/a3'])
    expect(entries?.[1].sizeBytes).toBe(5)
    expect(patchEntries(pane, patch({ path: '/elsewhere' }), PATCH_STATE)).toBeNull()
  })

  it('re-sorts by size and appends for the items key', () => {
    const entries = [fileEntry('/root/small', { sizeBytes: 1 })]
    const added = { path: '/root/big', entry: fileEntry('/root/big', { sizeBytes: 9 }) }
    const bySize = paneWith('left', {
      path: '/root',
      sortKey: 'size',
      sortDirection: 'desc',
      entries,
    })
    const byItems = paneWith('left', { path: '/root', sortKey: 'items', entries })
    const event = patch({ changed: [added] })
    expect(patchEntries(bySize, event, PATCH_STATE)?.map((entry) => entry.path)).toEqual([
      '/root/big',
      '/root/small',
    ])
    expect(patchEntries(byItems, event, PATCH_STATE)?.map((entry) => entry.path)).toEqual([
      '/root/small',
      '/root/big',
    ])
  })

  it('skips patches while typing, for inactive tabs and for other paths', () => {
    const typing = panesDataWithLeft({ path: '/root', typing: true })
    expect(dirPatchOutcome(typing, 'left', patch(), TAB_ID)).toEqual({
      kind: 'skipped',
      reason: 'typing',
    })
    const idle = panesDataWithLeft({ path: '/root' })
    expect(dirPatchOutcome(idle, 'left', patch(), 'other-tab')).toEqual({
      kind: 'skipped',
      reason: 'inactiveTab',
    })
    expect(dirPatchOutcome(idle, 'left', patch({ path: '/elsewhere' }), TAB_ID)).toEqual({
      kind: 'skipped',
      reason: 'pathMismatch',
    })
    const items = panesDataWithLeft({ path: '/root', sortKey: 'items' })
    expect(dirPatchOutcome(items, 'left', patch({ path: '/elsewhere' }), TAB_ID).kind).toBe(
      'skipped',
    )
  })

  it('applies rows, keeps focus and folds folder changes into a loaded tree node', () => {
    const state = panesDataWithLeft(
      { path: '/root', entries: [fileEntry('/root/a')], focusedEntryId: '/root/a' },
      { treeNodes: { '/root': loadedNode('/root', []) } },
    )
    const event = patch({ changed: [{ path: '/root/dir', entry: directoryEntry('/root/dir') }] })
    const outcome = dirPatchOutcome(state, 'left', event, TAB_ID)
    expect(outcome.kind).toBe('applied')
    if (outcome.kind !== 'applied') return
    expect(outcome.entryCount).toBe(2)
    expect(outcome.next.panes?.left.focusedEntryId).toBe('/root/a')
    expect(outcome.next.treeNodes?.['/root'].children).toEqual(['/root/dir'])
  })

  it('marks an items-sorted pane as counting without touching its rows', () => {
    const entries = [fileEntry('/root/a')]
    const state = panesDataWithLeft({ path: '/root', sortKey: 'items', entries })
    const outcome = dirPatchOutcome(state, 'left', patch({ removed: ['/root/a'] }), TAB_ID)
    expect(outcome.kind).toBe('itemsSortPending')
    if (outcome.kind !== 'itemsSortPending') return
    expect(outcome.next.panes?.left.entries).toBe(entries)
    expect(outcome.next.panes?.left.itemsSortStatus).toBe('counting')
    expect(outcome.next.treeNodes).toBeUndefined()
  })
})
