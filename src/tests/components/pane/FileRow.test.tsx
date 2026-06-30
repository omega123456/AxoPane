import { render, screen } from '@testing-library/react'
import { afterEach, beforeEach, vi } from 'vitest'
import { FileRow } from '@/components/pane/FileRow'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'

function makeEntry(overrides: Partial<DirectoryEntry> = {}): DirectoryEntry {
  return {
    id: 'doc',
    name: 'Doc.txt',
    path: 'C:\\root\\Doc.txt',
    isDir: false,
    sizeBytes: 2048,
    itemCount: null,
    typeLabel: 'TXT file',
    modifiedAt: '2026-06-20T14:05:09Z',
    createdAt: '2026-06-01T09:00:00Z',
    attributes: [],
    isHidden: false,
    isSystem: false,
    ...overrides,
  }
}

const noopHandlers = {
  onPointerDown: vi.fn(),
  onActivate: vi.fn(),
  onClick: vi.fn(),
  onContextMenu: vi.fn(),
  onMiddleClick: vi.fn(),
}

beforeEach(() => {
  useLayoutStore.getState().reset()
  useConfigStore.getState().reset()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('FileRow date column', () => {
  it('renders the configured absolute format', () => {
    useConfigStore.setState({ dateFormat: 'dmy', relativeDates: false })

    render(
      <FileRow
        entry={makeEntry()}
        isActivePane
        isFocused={false}
        isSelected={false}
        {...noopHandlers}
      />,
    )

    expect(screen.getByText('20/06/2026')).toBeInTheDocument()
  })

  it('colour-codes a recent modified date when relative dates are enabled', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-30T12:00:00Z'))
    useConfigStore.setState({ dateFormat: 'ymd', relativeDates: true })

    render(
      <FileRow
        entry={makeEntry({ modifiedAt: '2026-06-30T10:00:00Z' })}
        isActivePane
        isFocused={false}
        isSelected={false}
        {...noopHandlers}
      />,
    )

    const cell = screen.getByText('2 hours ago')
    expect(cell).toHaveClass('text-accent-blue-light')
  })
})
