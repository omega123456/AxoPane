import { useState, type ComponentType, type ReactNode } from 'react'
import type { LucideProps } from 'lucide-react'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { FileCategory, FolderGlyphKind } from '@/lib/file-type'
import { getFileCategory, getFolderGlyphKind } from '@/lib/file-type'
import {
  CornerUpRightIcon,
  CpuIcon,
  DatabaseIcon,
  DiscIcon,
  FileArchiveIcon,
  FileAudioIcon,
  FileCode2Icon,
  FileCogIcon,
  FileIcon,
  FileImageIcon,
  FileTextIcon,
  FileTypeIcon,
  FileVideoIcon,
  FolderCogIcon,
  FolderDownIcon,
  FolderGit2Icon,
  FolderIcon,
  FolderOpenIcon,
  TypeIcon,
} from '@/components/icons'

type IconComponent = ComponentType<LucideProps>

type CategoryStyle = {
  Glyph: IconComponent
  colorClassName: string
}

/** Non-folder category → glyph + default color utility. */
const CATEGORY_STYLES: Record<Exclude<FileCategory, 'folder'>, CategoryStyle> = {
  code: { Glyph: FileCode2Icon, colorClassName: 'text-file-code' },
  web: { Glyph: FileCode2Icon, colorClassName: 'text-file-web' },
  config: { Glyph: FileCogIcon, colorClassName: 'text-accent-green' },
  text: { Glyph: FileTextIcon, colorClassName: 'text-file-doc' },
  office: { Glyph: FileTypeIcon, colorClassName: 'text-file-office' },
  image: { Glyph: FileImageIcon, colorClassName: 'text-file-image' },
  audio: { Glyph: FileAudioIcon, colorClassName: 'text-file-audio' },
  video: { Glyph: FileVideoIcon, colorClassName: 'text-file-video' },
  archive: { Glyph: FileArchiveIcon, colorClassName: 'text-file-archive' },
  disc: { Glyph: DiscIcon, colorClassName: 'text-file-iso' },
  executable: { Glyph: CpuIcon, colorClassName: 'text-accent-red' },
  font: { Glyph: TypeIcon, colorClassName: 'text-file-font' },
  database: { Glyph: DatabaseIcon, colorClassName: 'text-accent-green' },
  generic: { Glyph: FileIcon, colorClassName: 'text-light-text-muted dark:text-dark-text-muted' },
}

const FOLDER_GLYPHS: Record<FolderGlyphKind, IconComponent> = {
  downloads: FolderDownIcon,
  git: FolderGit2Icon,
  modules: FolderCogIcon,
  open: FolderOpenIcon,
  closed: FolderIcon,
}

const FOLDER_COLOR = 'text-accent-blue-light dark:text-accent-blue'

type EntryIconProps = {
  entry: Pick<DirectoryEntry, 'name' | 'isDir' | 'iconDataUrl'> & Partial<Pick<DirectoryEntry, 'attributes'>>
  /** Tree expansion / open state — drives `FolderOpen` and the fill opacity. */
  isOpen?: boolean
  /** Size / layout utilities. Defaults to a compact 16px square. */
  className?: string
  /**
   * Overrides the category's default color utility. Pass `''` to inherit the
   * surrounding `currentColor` (used by the tree to keep its node accent).
   */
  colorClassName?: string
}

/** Small corner badge marking a symlink/junction so it's visually distinct from a real folder or file. */
function SymlinkBadge() {
  return (
    <CornerUpRightIcon
      aria-hidden="true"
      strokeWidth={2.5}
      className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full bg-light-window text-light-text-muted dark:bg-dark-window dark:text-dark-text-muted"
    />
  )
}

/**
 * File-type-aware icon: distinct lucide glyph + token color per category, plus
 * folder polish (faint fill, open-state, name-based special-folder glyphs).
 * Classification is pure name parsing — see {@link getFileCategory}.
 */
export function EntryIcon({ entry, isOpen = false, className = 'h-4 w-4 shrink-0', colorClassName }: EntryIconProps) {
  const [nativeIconFailed, setNativeIconFailed] = useState(false)
  const category = getFileCategory(entry)
  const isSymlink = entry.attributes?.includes('symlink') ?? false

  let glyph: ReactNode

  if (!entry.isDir && entry.iconDataUrl && !nativeIconFailed) {
    glyph = (
      <img
        src={entry.iconDataUrl}
        alt=""
        aria-hidden="true"
        className={className}
        onError={() => {
          setNativeIconFailed(true)
        }}
      />
    )
  } else if (category === 'folder') {
    const kind = getFolderGlyphKind(entry.name, isOpen)
    const Glyph = FOLDER_GLYPHS[kind]
    const color = colorClassName ?? FOLDER_COLOR
    glyph = (
      <Glyph
        data-file-category="folder"
        className={`${className} ${color}`.trim()}
        fill="currentColor"
        fillOpacity={isOpen ? 0.26 : 0.18}
      />
    )
  } else {
    const { Glyph, colorClassName: defaultColor } = CATEGORY_STYLES[category]
    const color = colorClassName ?? defaultColor
    glyph = <Glyph data-file-category={category} className={`${className} ${color}`.trim()} />
  }

  if (!isSymlink) {
    return glyph
  }

  return (
    <span className="relative inline-flex shrink-0" data-symlink="true">
      {glyph}
      <SymlinkBadge />
    </span>
  )
}
