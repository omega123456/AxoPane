import type { NativeMenuCanonicalActionKind, NativeMenuItem } from '@/lib/types/ipc'

const APP_OWNED_CANONICAL_ACTION_KINDS = new Set<NativeMenuCanonicalActionKind>([
  'open',
  'openWith',
  'copy',
  'copyAsPath',
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
  'copyaspath',
  'cut',
  'paste',
  'rename',
  'delete',
  'deletepermanently',
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

    const isDuplicate = isAppOwnedDuplicate(nextItem) || seen.has(key)
    if (isDuplicate) {
      return []
    }
    seen.add(key)

    return [nextItem]
  })
}

function isAppOwnedDuplicate(item: NativeMenuItem): boolean {
  const normalizedVerb = normalizeVerb(item.normalizedVerb ?? item.label)
  const isShellOwnedSubmenu =
    hasChildren(item) && !isAlwaysAppOwnedSubmenu(item.canonicalActionKind, normalizedVerb)
  if (isShellOwnedSubmenu) {
    return false
  }

  if (isAppOwnedCanonicalKind(item.canonicalActionKind)) {
    return true
  }

  return isOpenWithVariant(normalizedVerb) || APP_OWNED_NORMALIZED_VERBS.has(normalizedVerb)
}

function hasChildren(item: NativeMenuItem) {
  return item.children.length > 0
}

function isAppOwnedCanonicalKind(canonicalActionKind: NativeMenuItem['canonicalActionKind']) {
  if (!canonicalActionKind) {
    return false
  }
  return APP_OWNED_CANONICAL_ACTION_KINDS.has(canonicalActionKind)
}

function isAlwaysAppOwnedSubmenu(
  canonicalActionKind: NativeMenuItem['canonicalActionKind'],
  normalizedVerb: string,
) {
  return canonicalActionKind === 'openWith' || isOpenWithVariant(normalizedVerb)
}

function isOpenWithVariant(normalizedVerb: string) {
  return normalizedVerb.startsWith('openwith')
}

function normalizeVerb(value: string): string {
  return value.replace(/[^a-z0-9]/gi, '').toLowerCase()
}

function dedupeKey(item: NativeMenuItem): string {
  return [
    item.canonicalActionKind ?? '',
    normalizeVerb(item.label),
    hasChildren(item) ? 'submenu' : 'leaf',
  ].join(':')
}
