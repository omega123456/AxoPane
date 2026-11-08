import { pathsMatch } from '@/lib/path-compare'
import { clamp } from '@/lib/numbers'
import { moveItem } from '@/lib/collections'

export type FavouritesState = {
  favourites: string[]
  favouriteNames: Record<string, string>
}

export function withAddedFavourite(
  state: FavouritesState,
  path: string,
  index?: number,
): Pick<FavouritesState, 'favourites'> | null {
  if (state.favourites.some((item) => pathsMatch(item, path))) {
    return null
  }
  const favourites = [...state.favourites]
  favourites.splice(clamp(index ?? favourites.length, 0, favourites.length), 0, path)
  return { favourites }
}

export function withoutFavourite(state: FavouritesState, path: string): FavouritesState | null {
  const favourites = state.favourites.filter((item) => !pathsMatch(item, path))
  if (favourites.length === state.favourites.length) {
    return null
  }
  const favouriteNames = Object.fromEntries(
    Object.entries(state.favouriteNames).filter(([key]) => favourites.includes(key)),
  )
  return { favourites, favouriteNames }
}

export function withRenamedFavourite(
  state: FavouritesState,
  path: string,
  name: string,
): Pick<FavouritesState, 'favouriteNames'> | null {
  const trimmed = name.trim()
  const { [path]: previous, ...rest } = state.favouriteNames
  const nextName = trimmed || undefined
  if (nextName === previous) {
    return null
  }
  return { favouriteNames: trimmed ? { ...rest, [path]: trimmed } : rest }
}

export function withReorderedFavourite(
  state: FavouritesState,
  path: string,
  index: number,
): Pick<FavouritesState, 'favourites'> | null {
  const sourceIndex = state.favourites.findIndex((item) => pathsMatch(item, path))
  if (sourceIndex === -1) {
    return null
  }
  const adjustedIndex = sourceIndex < index ? index - 1 : index
  const targetIndex = clamp(adjustedIndex, 0, state.favourites.length - 1)
  if (sourceIndex === targetIndex) {
    return null
  }
  return { favourites: moveItem(state.favourites, sourceIndex, targetIndex) }
}
