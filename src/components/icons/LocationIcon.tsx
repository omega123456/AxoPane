import type { VolumeInfo } from '@/lib/types/ipc'
import { findVolumeForPath, isVolumeRoot } from '@/lib/volumes'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { VolumeIcon } from '@/components/icons/VolumeIcon'
import { Trash2Icon } from '@/components/icons'
import { isTrashPath } from '@/lib/trash'

function lastSegment(path: string) {
  return (
    path
      .replace(/[\\/]+$/, '')
      .split(/[\\/]/)
      .filter(Boolean)
      .at(-1) ?? path
  )
}

type LocationIconProps = {
  /** The location the tab points at — a drive root, network share, or folder. */
  path: string
  volumes: VolumeInfo[]
  /** Size / layout utilities. Defaults to a compact icon. */
  className?: string
}

/**
 * Picks the glyph that best represents a tab's current location, reusing the
 * folder-tree icons so colors match exactly: a drive-type glyph (fixed /
 * removable / network) when the path is a volume root, otherwise a folder glyph
 * with special-folder polish (downloads / git / modules).
 */
export function LocationIcon({
  path,
  volumes,
  className = 'h-3.5 w-3.5 shrink-0',
}: LocationIconProps) {
  if (isTrashPath(path)) {
    return <Trash2Icon className={className} />
  }

  const volume = findVolumeForPath(path, volumes)
  if (volume && isVolumeRoot(path, volume)) {
    return <VolumeIcon volume={volume} className={className} />
  }

  return <EntryIcon entry={{ name: lastSegment(path), isDir: true }} className={className} />
}
