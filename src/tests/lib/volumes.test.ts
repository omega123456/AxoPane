import {
  findVolumeForPath,
  groupVolumesByCategory,
  isNetworkPath,
  isVolumeRoot,
  volumeCategory,
} from '@/lib/volumes'
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

describe('findVolumeForPath', () => {
  const c = volume({ mountRoot: 'C:\\', label: 'Windows' })
  const mount = volume({ mountRoot: 'C:\\Mounted\\', label: 'Nested' })

  it('returns the volume containing a path', () => {
    expect(findVolumeForPath('C:\\Users\\me', [c])).toBe(c)
  })

  it('prefers the deepest mount root when volumes are nested', () => {
    expect(findVolumeForPath('C:\\Mounted\\data', [c, mount])).toBe(mount)
    expect(findVolumeForPath('C:\\Users', [c, mount])).toBe(c)
  })

  it('returns null when no volume matches', () => {
    expect(findVolumeForPath('D:\\x', [c])).toBeNull()
  })
})

describe('isNetworkPath', () => {
  it('uses the matching mounted volume on both supported platforms', () => {
    expect(isNetworkPath('Z:\\shared\\report.txt', [volume({ mountRoot: 'Z:\\', isNetwork: true })])).toBe(
      true,
    )
    expect(
      isNetworkPath('/Volumes/team/project/report.txt', [
        volume({ mountRoot: '/Volumes/team', isNetwork: true }),
      ]),
    ).toBe(true)
    expect(isNetworkPath('C:\\Users\\me\\report.txt', [volume()])).toBe(false)
  })

  it('recognizes direct and extended UNC paths before volume discovery completes', () => {
    expect(isNetworkPath('\\\\server\\share\\report.txt', [])).toBe(true)
    expect(isNetworkPath('\\\\?\\UNC\\server\\share\\report.txt', [])).toBe(true)
    expect(isNetworkPath('\\\\?\\C:\\Users\\me\\report.txt', [])).toBe(false)
  })
})

describe('isVolumeRoot', () => {
  it('matches a drive root regardless of trailing separator or case', () => {
    const c = volume({ mountRoot: 'C:\\' })
    expect(isVolumeRoot('C:\\', c)).toBe(true)
    expect(isVolumeRoot('c:', c)).toBe(true)
    expect(isVolumeRoot('C:\\Users', c)).toBe(false)
  })

  it('matches a posix and network root', () => {
    expect(isVolumeRoot('/', volume({ mountRoot: '/' }))).toBe(true)
    const share = volume({ mountRoot: '\\\\server\\share', isNetwork: true })
    expect(isVolumeRoot('\\\\server\\share\\', share)).toBe(true)
    expect(isVolumeRoot('\\\\server\\share\\sub', share)).toBe(false)
  })
})
