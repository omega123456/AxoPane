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
      iconClass('\\\\server\\share', [
        volume({ mountRoot: '\\\\server\\share', isNetwork: true }),
      ]),
    ).toContain('lucide-network')
  })

  it('shows a folder glyph for a folder inside a volume', () => {
    expect(iconClass('C:\\Users\\me', [volume()])).toContain('lucide-folder')
  })

  it('shows special-folder glyphs by name', () => {
    expect(iconClass('C:\\Users\\me\\Downloads', [volume()])).toContain('lucide-folder-down')
    expect(iconClass('C:\\code\\repo\\.git', [volume()])).toContain('lucide-folder-git')
  })

  it('falls back to a plain folder when no volume is known', () => {
    expect(iconClass('C:\\anywhere', [])).toContain('lucide-folder')
  })

  it('shows the trash glyph for the trash pane, regardless of volumes', () => {
    expect(iconClass(TRASH_PATH, [volume()])).toContain('lucide-trash-2')
  })

  it('colors icons to match the folder tree', () => {
    expect(iconClass('C:\\', [volume()])).toContain('text-light-text-muted')
    expect(iconClass('E:\\', [volume({ mountRoot: 'E:\\', isRemovable: true })])).toContain(
      'text-accent-amber',
    )
    expect(
      iconClass('\\\\server\\share', [volume({ mountRoot: '\\\\server\\share', isNetwork: true })]),
    ).toContain('text-accent-green')
    expect(iconClass('C:\\Users\\me', [volume()])).toContain('text-accent-blue')
  })
})
