import type { DirectoryEntry, LoadNativeMenuRequest, NativeMenuTargetKind } from '@/lib/types/ipc'

export function extensionOf(path: string): string {
  const segments = path.split(/[/\\]/)
  const fileName = segments[segments.length - 1] as string

  const lastDotIndex = fileName.lastIndexOf('.')
  const lastCharacterIndex = fileName.length - 1
  const hasExtension = lastDotIndex > 0 && lastDotIndex < lastCharacterIndex
  if (!hasExtension) {
    return ''
  }

  return fileName.slice(lastDotIndex + 1).toLowerCase()
}

export function nativeMenuTypeKeyForEntry(entry: DirectoryEntry): string {
  const kind = entry.isDir ? 'folder' : 'file'
  return `${kind}::${extensionOf(entry.path)}`
}

export function buildWarmRequestForEntry(
  entry: DirectoryEntry,
  panePath: string,
  requestId: string,
): LoadNativeMenuRequest {
  const targetKind: NativeMenuTargetKind = entry.isDir ? 'folder' : 'file'

  return {
    requestId,
    targetKind,
    targetPath: entry.path,
    folderPath: panePath,
    selectedPaths: [entry.path],
  }
}
