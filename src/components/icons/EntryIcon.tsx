import { useState, type ComponentType } from 'react'
import type { LucideProps } from 'lucide-react'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { FileCategory } from '@/lib/file-type'
import { getFileCategory } from '@/lib/file-type'
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
  TypeIcon,
} from '@/components/icons'
import { FolderGlyph, PdfGlyph } from '@/components/icons/custom-icons'
import { TEXT_SECONDARY } from '@/lib/class-names'

type IconComponent = ComponentType<LucideProps>

type CategoryStyle = {
  Glyph: IconComponent
  colorClassName: string
}

/** Non-folder category → glyph + default color utility. */
const CATEGORY_STYLES: Record<Exclude<FileCategory, 'folder'>, CategoryStyle> = {
  code: { Glyph: FileCode2Icon, colorClassName: 'text-file-code' },
  web: { Glyph: FileCode2Icon, colorClassName: 'text-file-web' },
  config: { Glyph: FileCogIcon, colorClassName: 'text-file-config' },
  text: { Glyph: FileTextIcon, colorClassName: 'text-file-doc' },
  office: { Glyph: FileTypeIcon, colorClassName: 'text-file-office' },
  pdf: { Glyph: PdfGlyph, colorClassName: 'text-file-pdf' },
  image: { Glyph: FileImageIcon, colorClassName: 'text-file-image' },
  audio: { Glyph: FileAudioIcon, colorClassName: 'text-file-audio' },
  video: { Glyph: FileVideoIcon, colorClassName: 'text-file-video' },
  archive: { Glyph: FileArchiveIcon, colorClassName: 'text-file-archive' },
  disc: { Glyph: DiscIcon, colorClassName: 'text-file-iso' },
  executable: { Glyph: CpuIcon, colorClassName: 'text-file-exec' },
  font: { Glyph: TypeIcon, colorClassName: 'text-file-font' },
  database: { Glyph: DatabaseIcon, colorClassName: 'text-file-config' },
  generic: {
    Glyph: FileIcon,
    colorClassName: TEXT_SECONDARY,
  },
}

type EntryIconProps = {
  entry: Pick<DirectoryEntry, 'name' | 'isDir' | 'iconDataUrl'> &
    Partial<Pick<DirectoryEntry, 'attributes'>>
  /** Size / layout utilities. Defaults to a compact 16px square. */
  className?: string
  /**
   * Overrides a file category's default color utility. Pass `''` to inherit the
   * surrounding `currentColor`. Folders always use the two-tone folder glyph.
   */
  colorClassName?: string
}

/** Small corner badge marking a symlink/junction so it's visually distinct from a real folder or file. */
function SymlinkBadge() {
  return (
    <CornerUpRightIcon
      aria-hidden="true"
      strokeWidth={2.5}
      className={`absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full bg-light-base ${TEXT_SECONDARY} dark:bg-dark-base`}
    />
  )
}

type CategoryGlyphProps = {
  category: FileCategory
  className: string
  colorClassName?: string
}

function CategoryGlyph({ category, className, colorClassName }: CategoryGlyphProps) {
  if (category === 'folder') {
    return <FolderGlyph data-file-category="folder" className={className} />
  }
  const { Glyph, colorClassName: defaultColor } = CATEGORY_STYLES[category]
  const color = colorClassName ?? defaultColor
  return <Glyph data-file-category={category} className={`${className} ${color}`.trim()} />
}

function usableNativeIconUrl(entry: EntryIconProps['entry'], nativeIconFailed: boolean) {
  if (entry.isDir) {
    return null
  }
  if (nativeIconFailed) {
    return null
  }
  return entry.iconDataUrl || null
}

/**
 * File-type-aware icon: distinct lucide glyph + token color per category, plus
 * the filled two-tone folder glyph for every folder.
 * Classification is pure name parsing — see {@link getFileCategory}.
 */
export function EntryIcon({
  entry,
  className = 'h-4 w-4 shrink-0',
  colorClassName,
}: EntryIconProps) {
  const [nativeIconFailed, setNativeIconFailed] = useState(false)
  const category = getFileCategory(entry)
  const isSymlink = entry.attributes?.includes('symlink') ?? false
  const nativeIconUrl = usableNativeIconUrl(entry, nativeIconFailed)

  const glyph = nativeIconUrl ? (
    <img
      src={nativeIconUrl}
      alt=""
      aria-hidden="true"
      className={className}
      onError={() => setNativeIconFailed(true)}
    />
  ) : (
    <CategoryGlyph category={category} className={className} colorClassName={colorClassName} />
  )

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
