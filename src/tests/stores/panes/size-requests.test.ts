import { describe, expect, it } from 'vitest'
import type { SizeStateEvent } from '@/lib/types/ipc'
import {
  AUTO_FOLDER_SIZE_MAX_DIRECTORIES,
  appliedSizeStates,
  autoFolderSizeDisabledForPane,
  collectPendingSizeRequests,
  eagerSizesEnabled,
  refreshSizePaths,
  staleSizePathsOnLeave,
  summarizeSizeEvents,
  withoutStaleSizes,
  withPendingSizeRequests,
} from '@/stores/panes/size-requests'
import { directoryEntry, fileEntry, panesData, paneWith } from './pane-fixtures'

function sizeEvent(path: string, state: SizeStateEvent['state'], sizeBytes: number | null = null) {
  return { path, state, sizeBytes, source: 'manual' } satisfies SizeStateEvent
}

const READY = { state: 'ready' as const, sizeBytes: 4, source: 'manual' as const }
const CALCULATING = { state: 'calculating' as const, sizeBytes: null, source: 'manual' as const }

describe('size requests', () => {
  it('disables eager sizing above the per-pane folder limit', () => {
    const folders = Array.from({ length: AUTO_FOLDER_SIZE_MAX_DIRECTORIES + 1 }, (_, index) =>
      directoryEntry(`/d${index}`),
    )
    expect(autoFolderSizeDisabledForPane(folders)).toBe(true)
    expect(autoFolderSizeDisabledForPane([directoryEntry('/a'), fileEntry('/f')])).toBe(false)
  })

  it('needs an available index and the auto setting for eager sizes', () => {
    const available = { isAvailable: true } as never
    expect(eagerSizesEnabled(available, true)).toBe(true)
    expect(eagerSizesEnabled(available, false)).toBe(false)
    expect(eagerSizesEnabled(null, true)).toBe(false)
  })

  it('skips pending, calculating and settled paths and de-duplicates the rest', () => {
    const state = {
      pendingSizeRequests: { '/pending': true as const },
      sizeStates: { '/ready': READY, '/calculating': CALCULATING },
    }
    const paths = ['/new', '/new', '/pending', '/ready', '/calculating']
    expect(collectPendingSizeRequests(paths, state)).toEqual(['/new'])
    expect(withPendingSizeRequests(state, ['/new'], true).pendingSizeRequests).toEqual({
      '/pending': true,
      '/new': true,
    })
  })

  it('finds in-flight sizes the other pane is not showing and drops their markers', () => {
    const base = panesData({
      pendingSizeRequests: { '/pending': true },
      sizeStates: { '/calculating': CALCULATING, '/refreshing': { ...READY, refreshing: true } },
    })
    const leaving = ['/pending', '/calculating', '/refreshing', '/shared', '/idle'].map((path) =>
      directoryEntry(path),
    )
    const state = {
      ...base,
      panes: {
        left: paneWith('left', { entries: leaving }),
        right: paneWith('right', { entries: [directoryEntry('/shared')] }),
      },
      sizeStates: { ...base.sizeStates, '/shared': CALCULATING },
    }
    const stale = staleSizePathsOnLeave(state, 'left')
    expect(stale).toEqual(['/pending', '/calculating', '/refreshing'])

    const cleared = withoutStaleSizes(state, stale)
    expect(cleared.pendingSizeRequests).toEqual({})
    expect(cleared.sizeStates['/calculating']).toBeUndefined()
    expect(cleared.sizeStates['/refreshing']?.refreshing).toBe(false)
  })

  it('keeps a known value visible while it recalculates and settles terminal paths', () => {
    const state = {
      ...panesData(),
      pendingSizeRequests: { '/a': true as const, '/b': true as const },
      sizeStates: { '/a': READY },
    }
    const events = [sizeEvent('/a', 'calculating'), sizeEvent('/b', 'ready', 9)]
    const next = appliedSizeStates(state, events)
    expect(next.sizeStates?.['/a']).toEqual({ ...READY, refreshing: true })
    expect(next.sizeStates?.['/b']?.sizeBytes).toBe(9)
    expect(next.pendingSizeRequests).toEqual({ '/a': true })
    expect(appliedSizeStates(state, [])).toEqual({})
    expect(appliedSizeStates(state, [sizeEvent('/a', 'calculating')]).pendingSizeRequests).toBe(
      state.pendingSizeRequests,
    )
  })

  it('summarizes calculating and terminal events for diagnostics', () => {
    const events = [
      sizeEvent('/a', 'calculating'),
      sizeEvent('/b', 'na'),
      sizeEvent('/c', 'unknown'),
    ]
    expect(summarizeSizeEvents(events)).toEqual({
      batchSize: 3,
      calculatingCount: 1,
      terminalCount: 1,
    })
  })

  it('refreshes every folder when eager, otherwise only those with a result', () => {
    const sizeStates = { '/a': READY, '/b': CALCULATING }
    expect(refreshSizePaths(['/a', '/b', '/c'], sizeStates, true)).toEqual(['/a', '/b', '/c'])
    expect(refreshSizePaths(['/a', '/b', '/c'], sizeStates, false)).toEqual(['/a'])
  })
})
