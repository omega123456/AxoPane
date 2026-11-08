export type NativeMenuTargetKind =
  | 'file'
  | 'folder'
  | 'multi'
  | 'mixed'
  | 'driveRoot'
  | 'background'
  | 'tree'
  | 'tab'

export type NativeMenuCanonicalActionKind =
  | 'open'
  | 'openWith'
  | 'copy'
  | 'copyAsPath'
  | 'cut'
  | 'paste'
  | 'rename'
  | 'delete'
  | 'properties'
  | 'share'
  | 'compress'
  | 'extract'
  | 'refresh'
  | 'newFolder'
  | 'newFile'
  | 'selectAll'

export type NativeMenuIconKind = 'dataUrl'

export type NativeMenuIcon = {
  kind: NativeMenuIconKind
  dataUrl: string
  alt?: string | null
}

export type NativeMenuItem = {
  id: string
  label: string
  enabled: boolean
  danger: boolean
  canonicalActionKind: NativeMenuCanonicalActionKind | null
  normalizedVerb: string | null
  invokeToken: string | null
  icon: NativeMenuIcon | null
  children: NativeMenuItem[]
}

export type LoadNativeMenuRequest = {
  requestId: string
  targetKind: NativeMenuTargetKind
  targetPath: string | null
  folderPath: string | null
  selectedPaths: string[]
}

export type LoadNativeMenuResponse = {
  requestId: string
  items: NativeMenuItem[]
}

/**
 * Batch of representative single-item native menu requests used to
 * proactively warm the backend's native-menu cache in the background. Each
 * element is a full `LoadNativeMenuRequest` (not the context-menu module's
 * `Omit<LoadNativeMenuRequest, 'requestId'>` request type) so it can carry a
 * throwaway `requestId`.
 */
export type WarmNativeMenusRequest = {
  requests: LoadNativeMenuRequest[]
}

export type InvokeNativeMenuRequest = {
  token: string
}

export type ShowPropertiesRequest = {
  paths: string[]
}

export type OpenWithRequest = {
  path: string
}

export type MacApp = {
  name: string
  bundlePath: string
  bundleId: string | null
  iconDataUrl: string | null
}

export type ListApplicationsResponse = {
  apps: MacApp[]
}

export type SetDefaultApplicationRequest = {
  path: string
  bundlePath: string
}

export type GetDefaultApplicationRequest = {
  path: string
}

export type GetDefaultApplicationResponse = {
  app: MacApp | null
}

export type CompressArchiveRequest = {
  paths: string[]
  destinationDir: string
}

export type ExtractArchiveRequest = {
  paths: string[]
  destinationDir: string
}
