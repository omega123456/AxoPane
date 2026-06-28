import type { NativeMenuCanonicalActionKind, NativeMenuItem } from '@/lib/types/ipc'

const APP_OWNED_CANONICAL_ACTION_KINDS = new Set<NativeMenuCanonicalActionKind>([
  'open',
  'openWith',
  'copy',
  'cut',
  'paste',
  'rename',
  'delete',
  'properties',
  'compress',
  'extract',
  'refresh',
  'newFolder',
  'newFile',
  'selectAll',
])

const APP_OWNED_NORMALIZED_VERBS = new Set([
  'open',
  'openwith',
  'copy',
  'cut',
  'paste',
  'rename',
  'delete',
  'properties',
  'compress',
  'extract',
  'refresh',
  'newfolder',
  'newfile',
  'selectall',
])

export function dedupeNativeMenuItems(items: NativeMenuItem[]): NativeMenuItem[] {
  const seen = new Set<string>()

  return items.flatMap((item) => {
    const children = dedupeNativeMenuItems(item.children)
    const nextItem = { ...item, children }
    const key = dedupeKey(nextItem)

    if (isAppOwnedDuplicate(nextItem) || seen.has(key)) {
      return []
    }
    seen.add(key)

    return [nextItem]
  })
}

function isAppOwnedDuplicate(item: NativeMenuItem): boolean {
  if (item.children.length > 0) {
    return false
  }

  if (item.canonicalActionKind && APP_OWNED_CANONICAL_ACTION_KINDS.has(item.canonicalActionKind)) {
    return true
  }

  const normalizedVerb = normalizeVerb(item.normalizedVerb ?? item.label)
  return APP_OWNED_NORMALIZED_VERBS.has(normalizedVerb)
}

function normalizeVerb(value: string): string {
  return value.replace(/[^a-z0-9]/gi, '').toLowerCase()
}

function dedupeKey(item: NativeMenuItem): string {
  return [
    item.canonicalActionKind ?? '',
    normalizeVerb(item.normalizedVerb ?? item.label),
    item.children.length > 0 ? 'submenu' : 'leaf',
  ].join(':')
}
