import { render } from '@testing-library/react'
import { LocationIcon } from '@/components/icons/LocationIcon'
import { TRASH_PATH } from '@/lib/trash'
import type { VolumeInfo } from '@/lib/types/ipc'

function volume(overrides: Partial<VolumeInfo> = {}): VolumeInfo {
  return {
    mountRoot: 'C:\\',
    label: 'Windows',
    totalBytes: 1,
    freeBytes: 1,
    isNetwork: false,
    isRemovable: false,
    ...overrides,
  }
}

function iconClass(path: string, volumes: VolumeInfo[]) {
  const { container } = render(<LocationIcon path={path} volumes={volumes} />)
  return container.querySelector('svg')?.getAttribute('class') ?? ''
}

/** Folders use the filled two-tone glyph rather than a Lucide outline. */
function isFolderGlyph(path: string, volumes: VolumeInfo[]) {
  const { container } = render(<LocationIcon path={path} volumes={volumes} />)
  return (
    container.querySelector('path.fill-folder-back') !== null &&
    container.querySelector('path.fill-folder') !== null
  )
}

describe('LocationIcon', () => {
  it('shows a hard-drive glyph at a fixed volume root', () => {
    expect(iconClass('C:\\', [volume()])).toContain('lucide-hard-drive')
  })

  it('shows a USB glyph at a removable volume root', () => {
    expect(iconClass('E:\\', [volume({ mountRoot: 'E:\\', isRemovable: true })])).toContain(
      'lucide-usb',
    )
  })

  it('shows a network glyph at a network share root', () => {
    expect(
      iconClass('\\\\server\\share', [volume({ mountRoot: '\\\\server\\share', isNetwork: true })]),
    ).toContain('lucide-network')
  })

  it('shows the two-tone folder glyph for a folder inside a volume', () => {
    expect(isFolderGlyph('C:\\Users\\me', [volume()])).toBe(true)
  })

  it('uses the same folder glyph for special folders', () => {
    expect(isFolderGlyph('C:\\Users\\me\\Downloads', [volume()])).toBe(true)
    expect(isFolderGlyph('C:\\code\\repo\\.git', [volume()])).toBe(true)
  })

  it('falls back to a plain folder when no volume is known', () => {
    expect(isFolderGlyph('C:\\anywhere', [])).toBe(true)
  })

  it('shows the trash glyph for the trash pane, regardless of volumes', () => {
    expect(iconClass(TRASH_PATH, [volume()])).toContain('lucide-trash-2')
  })

  it('colors icons to match the folder tree', () => {
    expect(iconClass('C:\\', [volume()])).toContain('text-light-text-secondary')
    expect(iconClass('E:\\', [volume({ mountRoot: 'E:\\', isRemovable: true })])).toContain(
      'text-volume-removable',
    )
    expect(
      iconClass('\\\\server\\share', [volume({ mountRoot: '\\\\server\\share', isNetwork: true })]),
    ).toContain('text-volume-network')
  })
})
