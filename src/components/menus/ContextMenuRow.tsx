import { useState, type ComponentType, type Ref, type SVGProps } from 'react'
import {
  ArrowLeftIcon,
  ArrowRightIcon,
  ChevronRightIcon,
  CopyIcon,
  CpuIcon,
  EjectIcon,
  FileCogIcon,
  FilePlusIcon,
  FolderOpenIcon,
  FolderPlusIcon,
  InfoIcon,
  PackageIcon,
  PackageOpenIcon,
  PanelLeftIcon,
  PanelRightIcon,
  PlusIcon,
  RefreshIcon,
  RotateCcwIcon,
  ScissorsIcon,
  Share2Icon,
  SquareCheckIcon,
  Trash2Icon,
  TypeIcon,
  XIcon,
} from '@/components/icons'
import type {
  ContextMenuIcon,
  ContextMenuRowItem,
  ContextMenuSubmenuRow,
} from '@/lib/types/context-menu'

type IconComponent = ComponentType<SVGProps<SVGSVGElement>>

const appIconMap = {
  archive: PackageIcon,
  'arrow-left': ArrowLeftIcon,
  'arrow-right': ArrowRightIcon,
  'calculate-size': CpuIcon,
  'close-tab': XIcon,
  copy: CopyIcon,
  cut: ScissorsIcon,
  delete: Trash2Icon,
  eject: EjectIcon,
  'empty-trash': Trash2Icon,
  extract: PackageOpenIcon,
  'new-file': FilePlusIcon,
  'new-folder': FolderPlusIcon,
  open: FolderOpenIcon,
  'open-in-new-tab': PlusIcon,
  'open-in-other-pane': PanelLeftIcon,
  'open-in-left-pane': PanelLeftIcon,
  'open-in-right-pane': PanelRightIcon,
  'open-with': FileCogIcon,
  paste: CopyIcon,
  'panel-left': PanelLeftIcon,
  'panel-right': PanelRightIcon,
  properties: InfoIcon,
  refresh: RefreshIcon,
  rename: TypeIcon,
  restore: RotateCcwIcon,
  'select-all': SquareCheckIcon,
  share: Share2Icon,
} satisfies Record<NonNullable<Extract<ContextMenuIcon, { kind: 'app' }>['name']>, IconComponent>

function itemTextClass(
  item: Pick<ContextMenuRowItem | ContextMenuSubmenuRow, 'disabled' | 'danger'>,
) {
  if (item.disabled) {
    return 'text-light-text-faint dark:text-dark-text-faint'
  }

  if (item.danger) {
    return 'text-accent-red'
  }

  return 'text-light-text dark:text-dark-text'
}

export function ContextMenuIconGlyph({
  icon,
  className,
}: {
  icon: ContextMenuIcon
  className?: string
}) {
  const [nativeLoadFailed, setNativeLoadFailed] = useState(false)

  if (icon.kind === 'native') {
    if (nativeLoadFailed) {
      return null
    }

    return (
      <img
        src={icon.dataUrl}
        alt={icon.alt ?? ''}
        aria-hidden={icon.alt ? undefined : true}
        className={className}
        onError={() => {
          setNativeLoadFailed(true)
        }}
      />
    )
  }

  const Icon = appIconMap[icon.name]
  return <Icon className={className} aria-hidden="true" />
}

export function ContextMenuRow({
  item,
  active,
  submenuOpen = false,
  onPointerEnter,
  onActivate,
  buttonRef,
}: {
  item: ContextMenuRowItem | ContextMenuSubmenuRow
  active: boolean
  submenuOpen?: boolean
  onPointerEnter: () => void
  onActivate: () => void
  buttonRef?: Ref<HTMLButtonElement>
}) {
  const iconClass = item.disabled
    ? 'text-light-text-faint dark:text-dark-text-faint'
    : 'text-light-text-muted dark:text-dark-text-muted'
  const hasSubmenu = 'kind' in item && item.kind === 'submenu'

  return (
    <div className="relative">
      <button
        ref={buttonRef}
        type="button"
        role="menuitem"
        aria-haspopup={hasSubmenu ? 'menu' : undefined}
        aria-expanded={hasSubmenu ? submenuOpen : undefined}
        disabled={item.disabled}
        className={`flex h-8 w-full items-center gap-3 rounded-lg px-2.5 text-left text-row focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border ${
          active ? 'bg-accent-blue-soft' : 'hover:bg-light-hover dark:hover:bg-dark-hover'
        } ${itemTextClass(item)} ${item.strong ? 'font-semibold' : ''}`}
        onMouseEnter={() => {
          if (!item.disabled) {
            onPointerEnter()
          }
        }}
        onClick={() => {
          if (!item.disabled) {
            onActivate()
          }
        }}
      >
        {item.icon ? (
          <span className={`flex size-4 flex-none items-center justify-center ${iconClass}`}>
            <ContextMenuIconGlyph icon={item.icon} className="size-4" />
          </span>
        ) : (
          <span className="size-4 flex-none" aria-hidden="true" />
        )}
        <span className="flex-1 truncate">{item.label}</span>
        {item.shortcut ? (
          <span className="flex-none font-mono text-2xs text-light-text-faint dark:text-dark-text-faint">
            {item.shortcut}
          </span>
        ) : null}
        {hasSubmenu ? (
          <ChevronRightIcon
            className={`size-3 flex-none ${
              submenuOpen
                ? 'text-accent-blue-light dark:text-accent-blue'
                : 'text-light-text-faint dark:text-dark-text-faint'
            }`}
          />
        ) : null}
      </button>
    </div>
  )
}
