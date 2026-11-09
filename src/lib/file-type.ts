import type { DirectoryEntry } from '@/lib/types/ipc'
import categoryExtensions from './file-type-extensions.json'

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

export const CATEGORY_EXTENSIONS: Record<
  Exclude<FileCategory, 'folder' | 'generic'>,
  string[]
> = categoryExtensions

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
  if (extensionStart <= 0) {
    return 'generic'
  }

  const extension = name.slice(extensionStart + 1)
  return EXTENSION_CATEGORY.get(extension) ?? 'generic'
}
