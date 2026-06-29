import type { ComponentType } from 'react'
import type { LucideProps } from 'lucide-react'
import type { VolumeCategory, VolumeInfo } from '@/lib/types/ipc'
import { volumeCategory } from '@/lib/volumes'
import { HardDriveIcon, NetworkIcon, UsbIcon } from '@/components/icons'

type IconComponent = ComponentType<LucideProps>

type VolumeStyle = {
  Glyph: IconComponent
  colorClassName: string
}

/**
 * Per-category drive glyph + token color. lucide has no OS-specific drive
 * glyph, so fixed disks (including the system drive) share {@link HardDriveIcon},
 * differentiated from removable/network media by shape and color.
 */
const VOLUME_STYLES: Record<VolumeCategory, VolumeStyle> = {
  fixed: { Glyph: HardDriveIcon, colorClassName: 'text-light-text-muted dark:text-dark-text-muted' },
  removable: { Glyph: UsbIcon, colorClassName: 'text-accent-amber' },
  network: { Glyph: NetworkIcon, colorClassName: 'text-accent-green' },
}

type VolumeIconProps = {
  volume: VolumeInfo
  /** Size / layout utilities. Defaults to a compact 16px square. */
  className?: string
}

/** Drive-type-aware icon for volume roots in the folder tree. */
export function VolumeIcon({ volume, className = 'h-4 w-4 shrink-0' }: VolumeIconProps) {
  const category = volumeCategory(volume)
  const { Glyph, colorClassName } = VOLUME_STYLES[category]
  return <Glyph data-volume-category={category} className={`${className} ${colorClassName}`} />
}
