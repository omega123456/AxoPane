import { render, screen } from '@testing-library/react'
import { beforeEach } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { FolderTree } from '@/components/tree/FolderTree'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
})

function seedVolumes() {
  usePanesStore.getState().initialize({
    session: { activePane: 'left', leftPath: '.', rightPath: '.' },
    showHiddenFiles: false,
    everythingStatus: { status: 'unavailable', isAvailable: false },
    volumes: [
      { mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false },
      { mountRoot: 'E:\\', label: 'USB Stick', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: true },
      { mountRoot: 'Z:\\', label: 'Share', totalBytes: 1, freeBytes: 1, isNetwork: true, isRemovable: false },
    ],
  })
}

describe('FolderTree', () => {
  it('groups volumes under Drives, Removable Drives, and Network Drives headings', () => {
    seedVolumes()
    render(<FolderTree />)

    expect(screen.getByText('Drives')).toBeInTheDocument()
    expect(screen.getByText('Removable Drives')).toBeInTheDocument()
    expect(screen.getByText('Network Drives')).toBeInTheDocument()

    expect(screen.getByText('Windows (C:)')).toBeInTheDocument()
    expect(screen.getByText('USB Stick (E:)')).toBeInTheDocument()
    expect(screen.getByText('Share (Z:)')).toBeInTheDocument()
  })

  it('omits category headings that have no volumes', () => {
    usePanesStore.getState().initialize({
      session: { activePane: 'left', leftPath: '.', rightPath: '.' },
      showHiddenFiles: false,
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [
        { mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false },
      ],
    })
    render(<FolderTree />)

    expect(screen.getByText('Drives')).toBeInTheDocument()
    expect(screen.queryByText('Removable Drives')).not.toBeInTheDocument()
    expect(screen.queryByText('Network Drives')).not.toBeInTheDocument()
  })
})
