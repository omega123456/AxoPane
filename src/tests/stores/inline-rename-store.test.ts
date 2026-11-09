import { describe, expect, it } from 'vitest'
import {
  renameDraftForEntry,
  withRenameDraft,
  type InlineRenameState,
} from '@/stores/inline-rename-store'

const rename: InlineRenameState = {
  paneId: 'left',
  entryId: 'Alpha',
  path: '/root/Alpha',
  initialValue: 'Alpha',
  value: 'Alpha 2',
  busy: true,
  error: 'Name exists',
}

describe('renameDraftForEntry', () => {
  it('hands the rename draft only to the entry being renamed', () => {
    expect(renameDraftForEntry(rename, 'Alpha')).toMatchObject({
      value: 'Alpha 2',
      busy: true,
      error: 'Name exists',
    })
    expect(renameDraftForEntry(rename, 'Beta')).toBeUndefined()
    expect(renameDraftForEntry(null, 'Alpha')).toBeUndefined()
  })
})

describe('withRenameDraft', () => {
  it('merges the draft fields into an active rename', () => {
    expect(withRenameDraft({ rename }, { value: 'Beta', error: null })).toEqual({
      rename: { ...rename, value: 'Beta', error: null },
    })
  })

  it('keeps no rename when none is active', () => {
    expect(withRenameDraft({ rename: null }, { busy: true })).toEqual({ rename: null })
  })
})
