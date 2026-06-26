import { groupVolumesByCategory, volumeCategory } from '@/lib/volumes'
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

describe('volumeCategory', () => {
  it('classifies fixed, removable, and network volumes', () => {
    expect(volumeCategory(volume())).toBe('fixed')
    expect(volumeCategory(volume({ isRemovable: true }))).toBe('removable')
    expect(volumeCategory(volume({ isNetwork: true }))).toBe('network')
  })

  it('treats a network mount as network even when it also reports removable', () => {
    expect(volumeCategory(volume({ isNetwork: true, isRemovable: true }))).toBe('network')
  })
})

describe('groupVolumesByCategory', () => {
  it('orders sections Drives → Removable → Network and only keeps non-empty ones', () => {
    const groups = groupVolumesByCategory([
      volume({ mountRoot: 'Z:\\', label: 'Share', isNetwork: true }),
      volume({ mountRoot: 'E:\\', label: 'USB', isRemovable: true }),
      volume({ mountRoot: 'C:\\', label: 'Windows' }),
    ])

    expect(groups.map((group) => group.label)).toEqual([
      'Drives',
      'Removable Drives',
      'Network Drives',
    ])
    expect(groups[0].volumes.map((entry) => entry.mountRoot)).toEqual(['C:\\'])
    expect(groups[1].volumes.map((entry) => entry.mountRoot)).toEqual(['E:\\'])
    expect(groups[2].volumes.map((entry) => entry.mountRoot)).toEqual(['Z:\\'])
  })

  it('omits sections with no volumes', () => {
    const groups = groupVolumesByCategory([volume({ mountRoot: 'C:\\' })])
    expect(groups.map((group) => group.category)).toEqual(['fixed'])
  })

  it('returns nothing for an empty volume list', () => {
    expect(groupVolumesByCategory([])).toEqual([])
  })
})
