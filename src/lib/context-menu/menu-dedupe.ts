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
  const normalizedVerb = normalizeVerb(item.normalizedVerb ?? item.label)
  if (item.children.length > 0 && !isAlwaysAppOwnedSubmenu(item.canonicalActionKind, normalizedVerb)) {
    return false
  }

  if (item.canonicalActionKind && APP_OWNED_CANONICAL_ACTION_KINDS.has(item.canonicalActionKind)) {
    return true
  }

  return isOpenWithVariant(normalizedVerb) || APP_OWNED_NORMALIZED_VERBS.has(normalizedVerb)
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
  // Key on the visible label (not the verb): the same command surfaced by both
  // the classic IContextMenu and modern IExplorerCommand paths (e.g. "Edit with
  // Notepad++") carries different verbs but an identical label, and is a
  // duplicate from the user's perspective.
  return [
    item.canonicalActionKind ?? '',
    normalizeVerb(item.label),
    item.children.length > 0 ? 'submenu' : 'leaf',
  ].join(':')
}
