import type { DirectoryEntry } from '@/lib/types/ipc'
import categoryExtensions from './file-type-extensions.json'

/**
 * Coarse, presentation-only classification of a directory entry derived purely
 * from its name. There is no IO and no folder inspection: categories drive the
 * per-type glyph + color in {@link import('@/components/icons/EntryIcon')}.
 */
export type FileCategory =
  | 'folder'
  | 'code'
  | 'web'
  | 'config'
  | 'text'
  | 'office'
  | 'pdf'
  | 'image'
  | 'audio'
  | 'video'
  | 'archive'
  | 'disc'
  | 'executable'
  | 'font'
  | 'database'
  | 'generic'

/** Special-folder glyph kinds, resolved by name (never by contents). */
export const CATEGORY_EXTENSIONS: Record<
  Exclude<FileCategory, 'folder' | 'generic'>,
  string[]
> = categoryExtensions

/**
 * Extensionless names that nonetheless carry a well-known category. Matched
 * case-insensitively against the whole name (e.g. `Dockerfile`, `.gitignore`).
 */
const SPECIAL_NAMES = new Map<string, FileCategory>([
  ['.gitignore', 'config'],
  ['.gitattributes', 'config'],
  ['.dockerignore', 'config'],
  ['.editorconfig', 'config'],
  ['dockerfile', 'config'],
])

const EXTENSION_CATEGORY = new Map<string, FileCategory>()
for (const [category, extensions] of Object.entries(CATEGORY_EXTENSIONS) as [
  FileCategory,
  string[],
][]) {
  for (const extension of extensions) {
    EXTENSION_CATEGORY.set(extension, category)
  }
}

/**
 * Classify an entry by name alone. Folders short-circuit to `folder`; files are
 * matched on their last `.`-segment (so `archive.tar.gz` → `gz` → archive).
 * Extensionless and dotfile names fall back to a small known set, else generic.
 */
export function getFileCategory(entry: Pick<DirectoryEntry, 'name' | 'isDir'>): FileCategory {
  if (entry.isDir) {
    return 'folder'
  }

  const name = entry.name.toLowerCase()

  const special = SPECIAL_NAMES.get(name)
  if (special) {
    return special
  }

  const extensionStart = name.lastIndexOf('.')
  // No dot, or a leading dot with nothing after the basename (e.g. `.env` has
  // its category covered by SPECIAL_NAMES; a bare `.foo` is treated as generic).
  if (extensionStart <= 0) {
    return 'generic'
  }

  const extension = name.slice(extensionStart + 1)
  return EXTENSION_CATEGORY.get(extension) ?? 'generic'
}
