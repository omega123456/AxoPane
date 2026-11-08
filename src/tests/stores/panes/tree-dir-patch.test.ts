import { describe, expect, it } from 'vitest'
import type { TreeNodeState } from '@/stores/panes/pane-state'
import { applyTreeDirPatch } from '@/stores/panes/tree-dir-patch'
import { directoryEntry, fileEntry } from './pane-fixtures'

function node(path: string, overrides: Partial<TreeNodeState> = {}): TreeNodeState {
  return {
    id: path,
    name: path.split('/').at(-1) || path,
    path,
    parentPath: null,
    children: [],
    expanded: false,
    loaded: false,
    ...overrides,
  }
}

describe('tree dir patch', () => {
  it('folds pane patches into a loaded node and ignores unloaded ones', () => {
    const treeNodes = {
      '/root': node('/root', { loaded: true, children: ['/root/old'] }),
      '/root/old': node('/root/old', { parentPath: '/root', children: ['/root/old/deep'] }),
      '/root/old/deep': node('/root/old/deep', { parentPath: '/root/old' }),
    }
    const event = {
      tabId: 't',
      path: '/root',
      reason: 'watch' as const,
      removed: ['/root/old'],
      changed: [
        { path: '/root/zeta', entry: directoryEntry('/root/zeta') },
        { path: '/root/alpha', entry: directoryEntry('/root/alpha') },
        { path: '/root/file', entry: fileEntry('/root/file') },
        { path: '/root/.hidden', entry: directoryEntry('/root/.hidden', { isHidden: true }) },
      ],
    }
    const next = applyTreeDirPatch({ treeNodes, showHiddenFiles: false }, event)
    expect(next?.['/root'].children).toEqual(['/root/alpha', '/root/zeta'])
    expect(next?.['/root/old']).toBeUndefined()
    expect(next?.['/root/old/deep']).toBeUndefined()

    const fileOnly = { ...event, removed: [], changed: [event.changed[2]] }
    expect(applyTreeDirPatch({ treeNodes, showHiddenFiles: false }, fileOnly)).toBeNull()
    const unloaded = { '/root': node('/root') }
    expect(applyTreeDirPatch({ treeNodes: unloaded, showHiddenFiles: false }, event)).toBeNull()
    expect(applyTreeDirPatch({ treeNodes: {}, showHiddenFiles: false }, event)).toBeNull()
  })
})
