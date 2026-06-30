import { afterEach } from 'vitest'
import {
  canSetDefaultApplication,
  getDefaultApplication,
  listApplications,
  setDefaultApplication,
} from '@/lib/app-picker-commands'
import { ipc } from '@/tests/ipc-mock'
import type { PropertiesDialogItem } from '@/stores/properties-dialog-store'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

afterEach(() => {
  setPlatform(originalPlatform)
})

const baseFile: PropertiesDialogItem = {
  attributes: [],
  createdAt: null,
  id: 'report',
  isDir: false,
  isHidden: false,
  isSystem: false,
  itemCount: null,
  modifiedAt: null,
  name: 'Report.pdf',
  path: '/Users/example/Report.pdf',
  sizeBytes: 100,
  typeLabel: 'PDF document',
}

describe('canSetDefaultApplication', () => {
  it('is true for a single macOS file with an extension', () => {
    setPlatform('MacIntel')
    expect(canSetDefaultApplication([baseFile])).toBe(true)
  })

  it('is false on Windows', () => {
    setPlatform('Win32')
    expect(canSetDefaultApplication([baseFile])).toBe(false)
  })

  it('is false for folders', () => {
    setPlatform('MacIntel')
    expect(canSetDefaultApplication([{ ...baseFile, isDir: true }])).toBe(false)
  })

  it('is false for multi-selections', () => {
    setPlatform('MacIntel')
    expect(
      canSetDefaultApplication([baseFile, { ...baseFile, id: 'other', name: 'Other.pdf' }]),
    ).toBe(false)
  })

  it('is false when the name has no extension', () => {
    setPlatform('MacIntel')
    expect(canSetDefaultApplication([{ ...baseFile, name: 'README' }])).toBe(false)
  })

  it('is false for a leading-dot dotfile name', () => {
    setPlatform('MacIntel')
    expect(canSetDefaultApplication([{ ...baseFile, name: '.gitignore' }])).toBe(false)
  })

  it('is false when there is no selection at all', () => {
    setPlatform('MacIntel')
    expect(canSetDefaultApplication([])).toBe(false)
  })
})

describe('listApplications', () => {
  it('returns the apps from the IPC fixture', async () => {
    const apps = await listApplications()
    expect(apps.length).toBeGreaterThan(0)
    expect(apps[0]).toHaveProperty('bundlePath')
  })

  it('returns an empty list when the IPC call fails', async () => {
    ipc.override('list_applications', () => {
      throw new Error('boom')
    })

    const apps = await listApplications()
    expect(apps).toEqual([])
  })
})

describe('setDefaultApplication', () => {
  const app = {
    name: 'Fixture Preview',
    bundlePath: '/Applications/Fixture Preview.app',
    bundleId: 'com.example.fixture-preview',
    iconDataUrl: null,
  }

  it('resolves true when the backend reports handled', async () => {
    ipc.override('set_default_application', { handled: true, message: 'ok' })

    const result = await setDefaultApplication('/Users/example/Report.pdf', app)
    expect(result).toBe(true)
  })

  it('resolves false when the backend reports unhandled', async () => {
    ipc.override('set_default_application', { handled: false, message: 'unsupported' })

    const result = await setDefaultApplication('/Users/example/Report.pdf', app)
    expect(result).toBe(false)
  })

  it('resolves false when the IPC call throws', async () => {
    ipc.override('set_default_application', () => {
      throw new Error('boom')
    })

    const result = await setDefaultApplication('/Users/example/Report.pdf', app)
    expect(result).toBe(false)
  })
})

describe('getDefaultApplication', () => {
  it('returns the app from the IPC fixture', async () => {
    const result = await getDefaultApplication('/Users/example/Report.pdf')
    expect(result?.name).toBe('Fixture Preview')
  })

  it('returns null when there is no registered default', async () => {
    ipc.override('get_default_application', { app: null })

    const result = await getDefaultApplication('/Users/example/Report.pdf')
    expect(result).toBeNull()
  })

  it('returns null when the IPC call fails', async () => {
    ipc.override('get_default_application', () => {
      throw new Error('boom')
    })

    const result = await getDefaultApplication('/Users/example/Report.pdf')
    expect(result).toBeNull()
  })
})
