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
  path: string
  volumes: VolumeInfo[]
  className?: string
}

export function LocationIcon({
  path,
  volumes,
  className = 'h-3.5 w-3.5 shrink-0',
}: LocationIconProps) {
  if (isTrashPath(path)) {
    return <Trash2Icon className={className} />
  }

  const volume = findVolumeForPath(path, volumes)
  const isRootOfVolume = volume !== null && isVolumeRoot(path, volume)
  if (isRootOfVolume) {
    return <VolumeIcon volume={volume} className={className} />
  }

  return <EntryIcon entry={{ name: lastSegment(path), isDir: true }} className={className} />
}
