import type { ComponentType } from 'react'
import type { LucideProps } from 'lucide-react'
import type { VolumeCategory, VolumeInfo } from '@/lib/types/ipc'
import { volumeCategory } from '@/lib/volumes'
import { HardDriveIcon, NetworkIcon, UsbIcon } from '@/components/icons'
import { TEXT_SECONDARY } from '@/lib/class-names'

type IconComponent = ComponentType<LucideProps>

type VolumeStyle = {
  Glyph: IconComponent
  colorClassName: string
}

const VOLUME_STYLES: Record<VolumeCategory, VolumeStyle> = {
  fixed: {
    Glyph: HardDriveIcon,
    colorClassName: TEXT_SECONDARY,
  },
  removable: { Glyph: UsbIcon, colorClassName: 'text-volume-removable' },
  network: { Glyph: NetworkIcon, colorClassName: 'text-volume-network' },
}

type VolumeIconProps = {
  volume: VolumeInfo
  className?: string
}

export function VolumeIcon({ volume, className = 'h-4 w-4 shrink-0' }: VolumeIconProps) {
  const category = volumeCategory(volume)
  const { Glyph, colorClassName } = VOLUME_STYLES[category]
  return <Glyph data-volume-category={category} className={`${className} ${colorClassName}`} />
}
