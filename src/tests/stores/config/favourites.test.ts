import { describe, expect, it } from 'vitest'
import {
  withAddedFavourite,
  withoutFavourite,
  withRenamedFavourite,
  withReorderedFavourite,
  type FavouritesState,
} from '@/stores/config/favourites'

function favourites(paths: string[], favouriteNames: Record<string, string> = {}): FavouritesState {
  return { favourites: paths, favouriteNames }
}

describe('favourites transitions', () => {
  it('adds a new favourite at a clamped index and ignores duplicates', () => {
    expect(withAddedFavourite(favourites(['/a']), '/b')).toEqual({ favourites: ['/a', '/b'] })
    expect(withAddedFavourite(favourites(['/a']), '/b', -4)).toEqual({ favourites: ['/b', '/a'] })
    expect(withAddedFavourite(favourites(['/a']), '/a')).toBeNull()
  })

  it('removes a favourite together with its orphaned custom name', () => {
    const state = favourites(['/a', '/b'], { '/a': 'Alpha', '/b': 'Beta' })
    expect(withoutFavourite(state, '/a')).toEqual({
      favourites: ['/b'],
      favouriteNames: { '/b': 'Beta' },
    })
    expect(withoutFavourite(state, '/zz')).toBeNull()
  })

  it('renames, restores on a blank name, and reports unchanged names', () => {
    const state = favourites(['/a'], { '/a': 'Alpha' })
    expect(withRenamedFavourite(state, '/a', ' Apex ')).toEqual({
      favouriteNames: { '/a': 'Apex' },
    })
    expect(withRenamedFavourite(state, '/a', '  ')).toEqual({ favouriteNames: {} })
    expect(withRenamedFavourite(state, '/a', 'Alpha')).toBeNull()
    expect(withRenamedFavourite(favourites(['/a']), '/a', '')).toBeNull()
  })

  it('reorders relative to the drop slot and ignores no-op moves', () => {
    const state = favourites(['/a', '/b', '/c'])
    expect(withReorderedFavourite(state, '/a', 3)).toEqual({ favourites: ['/b', '/c', '/a'] })
    expect(withReorderedFavourite(state, '/c', 0)).toEqual({ favourites: ['/c', '/a', '/b'] })
    expect(withReorderedFavourite(state, '/a', 1)).toBeNull()
    expect(withReorderedFavourite(state, '/zz', 0)).toBeNull()
  })
})
