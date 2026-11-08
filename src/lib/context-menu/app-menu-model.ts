import { dispatchByKind, type KindHandlers } from '@/lib/kind-table'
import type { PlatformOs } from '@/lib/keymap'
import type { ContextMenuContent, ContextMenuTarget } from '@/lib/types/context-menu'
import type { PaneId } from '@/types/pane'
import {
  buildEmptyContent,
  buildFileOrFolderContent,
  buildMultiContent,
} from '@/lib/context-menu/pane-menu-content'
import { buildTabContent } from '@/lib/context-menu/tab-menu-content'
import { buildTreeContent } from '@/lib/context-menu/tree-menu-content'

const appMenuContentBuilders: KindHandlers<
  ContextMenuTarget,
  [paneId: PaneId, platform: PlatformOs],
  ContextMenuContent
> = {
  file: (target, paneId, platform) => buildFileOrFolderContent(paneId, target, platform),
  folder: (target, paneId, platform) => buildFileOrFolderContent(paneId, target, platform),
  multi: (_target, paneId, platform) => buildMultiContent(paneId, platform),
  empty: (_target, paneId, platform) => buildEmptyContent(paneId, platform),
  tree: (target, _paneId, platform) => buildTreeContent(target, platform),
  favourite: (target, _paneId, platform) => buildTreeContent(target, platform),
  tab: (target, paneId) => buildTabContent(paneId, target),
}

export function buildAppContextMenuContent(
  paneId: PaneId,
  target: ContextMenuTarget,
  platform: PlatformOs,
): ContextMenuContent {
  return dispatchByKind(appMenuContentBuilders, target, paneId, platform)
}
