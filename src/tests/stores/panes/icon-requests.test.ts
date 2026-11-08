import { describe, expect, it } from 'vitest'
import {
  appliedIconStates,
  collectPendingIconRequests,
  hydrateEntryIcon,
  withPendingIconRequests,
  withResolvedIcons,
} from '@/stores/panes/icon-requests'
import { directoryEntry, fileEntry, panesDataWithLeft } from './pane-fixtures'

const ICON = 'data:image/png;base64,AA=='
const NOW = 1_000

describe('icon requests', () => {
  it('requests only listed files with no icon, cached result or request in flight', () => {
    const entries = [
      fileEntry('/new'),
      fileEntry('/has-icon', { iconDataUrl: ICON }),
      fileEntry('/resolved'),
      fileEntry('/pending'),
      directoryEntry('/folder'),
    ]
    const state = {
      pendingIconRequests: { '/pending': true as const },
      resolvedIconPaths: { '/resolved': null },
    }
    const paths = ['/new', '/new', '/has-icon', '/resolved', '/pending', '/folder', '/missing']
    expect(collectPendingIconRequests(entries, paths, state)).toEqual(['/new'])
    expect(withPendingIconRequests(state, ['/pending'], false)).toEqual({
      pendingIconRequests: {},
    })
  })

  it('hydrates cached icons and keeps references when nothing changes', () => {
    const file = fileEntry('/file')
    const folder = directoryEntry('/folder')
    expect(hydrateEntryIcon(file, { '/file': ICON })).toEqual({ ...file, iconDataUrl: ICON })
    expect(hydrateEntryIcon(folder, { '/folder': ICON })).toBe(folder)
    expect(hydrateEntryIcon(file, {})).toBe(file)

    const entries = [file, folder]
    expect(withResolvedIcons(entries, {})).toBe(entries)
    expect(withResolvedIcons(entries, { '/file': ICON })[0].iconDataUrl).toBe(ICON)
  })

  it('applies icon results to rows, records misses and clears pending requests', () => {
    const state = panesDataWithLeft(
      { entries: [fileEntry('/a'), fileEntry('/b')] },
      { pendingIconRequests: { '/a': true, '/b': true } },
    )
    const events = [
      { path: '/a', iconDataUrl: ICON },
      { path: '/b', iconDataUrl: null },
    ]
    const next = appliedIconStates(state, { events, now: NOW, windowsPlatform: true })
    expect(next.panes?.left.entries[0].iconDataUrl).toBe(ICON)
    expect(next.pendingIconRequests).toEqual({})
    expect(next.resolvedIconPaths).toEqual({ '/a': ICON, '/b': null })
    expect(next.iconCacheTouched).toEqual({ '/a': NOW, '/b': NOW })
  })

  it('keeps the panes reference when no listed row matches and ignores empty batches', () => {
    const state = panesDataWithLeft({ entries: [fileEntry('/a')] })
    const input = {
      events: [{ path: '/elsewhere', iconDataUrl: ICON }],
      now: NOW,
      windowsPlatform: false,
    }
    expect(appliedIconStates(state, input).panes).toBe(state.panes)
    expect(appliedIconStates(state, { ...input, events: [] })).toEqual({})
  })
})
