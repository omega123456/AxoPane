import { beforeEach, describe, expect, it } from 'vitest'
import { MAX_RECENT_FOLDERS, useRecentFoldersStore } from '@/stores/recent-folders-store'

const store = () => useRecentFoldersStore.getState()

beforeEach(() => {
  store().hydrate([])
})

describe('recent folders store', () => {
  it('records the most recent folder first without duplicates', () => {
    store().record('C:\\Users\\alex')
    store().record('D:\\Photos')
    store().record('c:\\users\\ALEX')
    expect(store().folders).toEqual(['c:\\users\\ALEX', 'D:\\Photos'])
  })

  it('keeps the list unchanged when the folder is already first', () => {
    store().record('/Users/alex')
    const before = store().folders
    store().record('/Users/alex')
    expect(store().folders).toBe(before)
  })

  it('caps the list when recording and hydrating', () => {
    for (let index = 0; index < MAX_RECENT_FOLDERS + 5; index += 1) {
      store().record(`/folder-${index}`)
    }
    expect(store().folders).toHaveLength(MAX_RECENT_FOLDERS)
    expect(store().folders[0]).toBe(`/folder-${MAX_RECENT_FOLDERS + 4}`)

    store().hydrate(Array.from({ length: 30 }, (_, index) => `/saved-${index}`))
    expect(store().folders).toHaveLength(MAX_RECENT_FOLDERS)
  })
})
