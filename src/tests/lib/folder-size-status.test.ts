import { describe, expect, it } from 'vitest'
import { folderSizeStatusLabel, isIndexRunning } from '@/lib/folder-size-status'
import type { FolderSizeIndexStatus } from '@/lib/types/ipc'

function macStatus(state: FolderSizeIndexStatus['state']): FolderSizeIndexStatus {
  return { backend: 'macIndex', isAvailable: false, state, fileNamesAvailable: false }
}

describe('folder size status', () => {
  it('labels an index that is turned off', () => {
    expect(folderSizeStatusLabel(macStatus('off'))).toBe('Off')
  })

  it('treats every state except off as a running index', () => {
    expect(isIndexRunning(macStatus('off'))).toBe(false)
    expect(isIndexRunning(macStatus('notReady'))).toBe(true)
    expect(isIndexRunning(macStatus('unavailable'))).toBe(true)
  })
})
