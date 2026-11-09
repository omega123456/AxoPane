import {
  copyPathsContextAction,
  ejectVolumeContextAction,
  navigateContextAction,
  openPathInExplicitPaneContextAction,
  openPathInNewTabContextAction,
  propertiesContextAction,
  requestFolderSizeContextAction,
  renameFavouriteContextAction,
} from '@/lib/context-menu/context-menu-actions'
import { findVolumeForPath, isVolumeRoot } from '@/lib/volumes'
import {
  createPathPropertiesDialogItem,
  createTrashPropertiesDialogItem,
} from '@/lib/properties-commands'
import { isTrashPath } from '@/lib/trash'
import type {
  ContextMenuActionRow,
  ContextMenuContent,
  ContextMenuSection,
  ContextMenuTarget,
} from '@/lib/types/context-menu'
import { commandLabels, type PlatformOs } from '@/lib/keymap'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'

import {
  calculateSizeHidden,
  commandRow,
  customRow,
  favouriteRow,
  section,
  shortcutFor,
} from '@/lib/context-menu/menu-rows'

function buildTrashTreeContent(platform: PlatformOs): ContextMenuContent {
  return {
    topStrip: [],
    sections: [
      section('primary', [commandRow('emptyTrash', platform, { danger: true, strong: true })]),
      section('properties', [
        customRow({
          id: 'properties-tree-trash',
          label: 'Properties',
          action: propertiesContextAction([createTrashPropertiesDialogItem(platform)]),
          icon: { kind: 'app', name: 'properties' },
        }),
      ]),
    ],
  }
}

function buildTreeOpenTargetRows(path: string): ContextMenuActionRow[] {
  if (useLayoutStore.getState().defaultPaneMode === 'single') {
    return [
      customRow({
        id: `open-tree-tab-${path}`,
        label: commandLabels.openInNewTab,
        action: openPathInNewTabContextAction(path),
        icon: { kind: 'app', name: 'open-in-new-tab' },
      }),
    ]
  }

  return [
    customRow({
      id: `open-tree-right-${path}`,
      label: 'Open in right pane',
      action: openPathInExplicitPaneContextAction(path, 'right'),
      icon: { kind: 'app', name: 'open-in-right-pane' },
    }),
    customRow({
      id: `open-tree-left-${path}`,
      label: 'Open in left pane',
      action: openPathInExplicitPaneContextAction(path, 'left'),
      icon: { kind: 'app', name: 'open-in-left-pane' },
    }),
  ]
}

type TreeTarget = Extract<ContextMenuTarget, { kind: 'tree' | 'favourite' }>

function treePrimarySection(target: TreeTarget, platform: PlatformOs): ContextMenuSection {
  return section('primary', [
    customRow({
      id: `open-tree-${target.path}`,
      label: commandLabels.open,
      action: navigateContextAction(target.path, 'current-pane'),
      shortcut: shortcutFor('open', platform),
      strong: true,
      icon: { kind: 'app', name: 'open' },
    }),
    ...buildTreeOpenTargetRows(target.path),
  ])
}

function renameFavouriteRows(target: TreeTarget): ContextMenuActionRow[] {
  if (target.kind !== 'favourite') {
    return []
  }
  return [
    customRow({
      id: `rename-favourite-${target.path}`,
      label: 'Rename Favourite',
      action: renameFavouriteContextAction(target.path),
      icon: { kind: 'app', name: 'rename' },
    }),
  ]
}

function treeFooterSection(target: TreeTarget, platform: PlatformOs): ContextMenuSection {
  const volumes = usePanesStore.getState().volumes
  const networkNode = volumes.find((volume) =>
    target.path.toLowerCase().startsWith(volume.mountRoot.toLowerCase()),
  )?.isNetwork
  return section('footer', [
    favouriteRow(target.path, target.kind === 'favourite'),
    ...renameFavouriteRows(target),
    customRow({
      id: `copy-tree-path-${target.path}`,
      label: 'Copy as path',
      action: copyPathsContextAction([target.path]),
      icon: { kind: 'app', name: 'copy' },
    }),
    customRow({
      id: `calculate-tree-${target.path}`,
      label: commandLabels.calculateSize,
      action: requestFolderSizeContextAction(target.path),
      shortcut: shortcutFor('calculateSize', platform),
      hidden: calculateSizeHidden(target.path),
      disabled: Boolean(networkNode),
      icon: { kind: 'app', name: 'calculate-size' },
    }),
  ])
}

function ejectableVolumeAt(path: string, platform: PlatformOs) {
  if (platform !== 'macos') {
    return null
  }
  const volume = findVolumeForPath(path, usePanesStore.getState().volumes)
  if (!volume?.isRemovable) {
    return null
  }
  return isVolumeRoot(path, volume) ? volume : null
}

/**
 * Eject is macOS-only: on Windows a safe removal isn't possible from here (an
 * in-use volume can only be force-dismounted), so Windows users use the native
 * "Eject" entry in the shell context menu instead.
 */
function ejectSections(target: TreeTarget, platform: PlatformOs): ContextMenuSection[] {
  const volume = ejectableVolumeAt(target.path, platform)
  if (!volume) {
    return []
  }
  return [
    section('eject', [
      customRow({
        id: `eject-tree-${target.path}`,
        label: 'Eject',
        action: ejectVolumeContextAction(volume.mountRoot),
        icon: { kind: 'app', name: 'eject' },
      }),
    ]),
  ]
}

export function buildTreeContent(target: TreeTarget, platform: PlatformOs): ContextMenuContent {
  if (isTrashPath(target.path)) {
    return buildTrashTreeContent(platform)
  }

  return {
    topStrip: [],
    sections: [
      treePrimarySection(target, platform),
      treeFooterSection(target, platform),
      ...ejectSections(target, platform),
      section('properties', [
        customRow({
          id: `properties-tree-${target.path}`,
          label: 'Properties',
          action: propertiesContextAction([createPathPropertiesDialogItem(target.path)]),
          icon: { kind: 'app', name: 'properties' },
        }),
      ]),
    ],
  }
}
