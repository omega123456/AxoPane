import type { DirectoryEntry, LoadNativeMenuRequest, NativeMenuTargetKind } from '@/lib/types/ipc'

/**
 * Derives the native-menu cache "type key" for a directory entry, mirroring
 * the Rust `MenuCache` key derivation (`target_kind::sorted-lowercased-extensions`)
 * for the single-item `File`/`Folder` case exactly: `file::<ext>` / `folder::<ext>`.
 *
 * Extension rules match the Rust `extension_of` helper
 * (`src-tauri/src/native_menu/menu_cache.rs`) bit-for-bit:
 * - Only the final path component (after the last `/` or `\`) is considered.
 * - The extension is the substring after the *last* `.` in that component,
 *   lowercased.
 * - A leading-dot name (e.g. `.gitignore`) has an empty stem before the dot
 *   and is therefore treated as extensionless.
 * - A trailing-dot name (e.g. `archive.`) has an empty extension after the
 *   dot and is therefore treated as extensionless.
 * - A component with no `.` at all is extensionless.
 */
export function extensionOf(path: string): string {
  const segments = path.split(/[/\\]/)
  // `String.prototype.split` always returns at least one element, so the
  // final segment is always defined (possibly an empty string for a
  // trailing separator).
  const fileName = segments[segments.length - 1] as string

  const lastDotIndex = fileName.lastIndexOf('.')
  if (lastDotIndex <= 0 || lastDotIndex === fileName.length - 1) {
    return ''
  }

  return fileName.slice(lastDotIndex + 1).toLowerCase()
}

/** Derives the native-menu cache type key (`file::<ext>` / `folder::<ext>`) for an entry. */
export function nativeMenuTypeKeyForEntry(entry: DirectoryEntry): string {
  const kind = entry.isDir ? 'folder' : 'file'
  return `${kind}::${extensionOf(entry.path)}`
}

/**
 * Builds the representative single-item warm request for an entry within a
 * given pane path. This reproduces exactly the shape a real single-row
 * right-click on this entry would build (see `buildNativeMenuRequest` in
 * `src/components/menus/menu-definitions.ts`), so the warmed cache key equals
 * the interactive cache key.
 */
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
