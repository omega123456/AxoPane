import type {
  AddIndexExclusionResponse,
  FolderSizeIndexStatus,
  IndexExclusionsResponse,
  PickIndexExclusionsResponse,
} from '@/lib/types/ipc'

const BUILD_CACHE = '/Users/alex/Projects/app/build-cache'
const NODE_MODULES = '/Users/alex/Projects/web/node_modules'

/** The excluded folders the Search index screenshots start with. */
export const searchIndexExclusions = [BUILD_CACHE, NODE_MODULES]

/** The path typed into the add field for the validation-message screenshot. */
export const searchIndexTypedInsideExisting = `${NODE_MODULES}/react`

export const searchIndexUpToDate: FolderSizeIndexStatus = {
  backend: 'macIndex',
  isAvailable: true,
  state: 'upToDate',
  fileNamesAvailable: true,
}

export const searchIndexUpdating: FolderSizeIndexStatus = {
  backend: 'macIndex',
  isAvailable: true,
  state: 'updating',
  fileNamesAvailable: true,
}

export const searchIndexUnavailable: FolderSizeIndexStatus = {
  backend: 'macIndex',
  isAvailable: false,
  state: 'unavailable',
  fileNamesAvailable: false,
}

export const searchIndexInsideExisting: AddIndexExclusionResponse = {
  exclusions: searchIndexExclusions,
  outcomes: [
    { path: searchIndexTypedInsideExisting, outcome: 'insideExisting', detail: NODE_MODULES },
  ],
}

export const searchIndexAfterRemovingBuildCache: IndexExclusionsResponse = {
  exclusions: [NODE_MODULES],
}

const noPickedFolders: PickIndexExclusionsResponse = { paths: [] }

/** Defaults for the exclusion commands; scenarios override them per screenshot. */
export const searchIndexCommandFixtures = {
  add_index_exclusion: { exclusions: [], outcomes: [] } satisfies AddIndexExclusionResponse,
  remove_index_exclusion: { exclusions: [] } satisfies IndexExclusionsResponse,
  pick_index_exclusions: noPickedFolders,
  retry_folder_size_index: undefined,
}
