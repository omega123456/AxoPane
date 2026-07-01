import { beforeEach, vi } from 'vitest'
import { waitFor } from '@testing-library/react'
import { ipc } from '@/tests/ipc-mock'
import {
  canDropInto,
  isSameOrDescendant,
  performDrop,
  resolveDropKind,
  sameVolume,
  type DragPayload,
} from '@/lib/drag-drop'

function payload(overrides: Partial<DragPayload> = {}): DragPayload {
  return {
    sourcePaneId: 'left',
    sourceDir: 'C:\\root',
    items: [
      { id: 'a', name: 'Alpha.txt', path: 'C:\\root\\Alpha.txt', isDir: false, sizeBytes: 10 },
    ],
    ...overrides,
  }
}

beforeEach(() => {
  ipc.install()
})

describe('isSameOrDescendant', () => {
  it('treats an identical path as a match regardless of trailing separators or case', () => {
    expect(isSameOrDescendant('C:\\Root\\', 'c:\\root', 'windows')).toBe(true)
  })

  it('matches nested descendants but not sibling prefixes', () => {
    expect(isSameOrDescendant('C:\\root\\sub\\deep', 'C:\\root', 'windows')).toBe(true)
    // A shared textual prefix that is not a path boundary must not match.
    expect(isSameOrDescendant('C:\\rootstuff', 'C:\\root', 'windows')).toBe(false)
  })

  it('handles POSIX paths', () => {
    expect(isSameOrDescendant('/Users/me/docs', '/Users/me', 'macos')).toBe(true)
    expect(isSameOrDescendant('/Users/other', '/Users/me', 'macos')).toBe(false)
  })
})

describe('sameVolume', () => {
  it('compares Windows drive letters case-insensitively', () => {
    expect(sameVolume('C:\\a', 'c:\\b\\c', 'windows')).toBe(true)
    expect(sameVolume('C:\\a', 'D:\\a', 'windows')).toBe(false)
  })

  it('keys Windows UNC paths on the share root', () => {
    expect(sameVolume('\\\\server\\share\\a', '\\\\server\\share\\b', 'windows')).toBe(true)
    expect(sameVolume('\\\\server\\share\\a', '\\\\server\\other\\b', 'windows')).toBe(false)
  })

  it('treats macOS mounts under /Volumes as distinct volumes', () => {
    expect(sameVolume('/Users/me', '/Applications', 'macos')).toBe(true)
    expect(sameVolume('/Volumes/USB/a', '/Users/me', 'macos')).toBe(false)
    expect(sameVolume('/Volumes/USB/a', '/Volumes/USB/b', 'macos')).toBe(true)
  })
})

describe('resolveDropKind', () => {
  it('defaults to move within a volume and copy across volumes', () => {
    expect(resolveDropKind({ ctrlKey: false, shiftKey: false }, 'C:\\a', 'C:\\b', 'windows')).toBe(
      'move',
    )
    expect(resolveDropKind({ ctrlKey: false, shiftKey: false }, 'C:\\a', 'D:\\b', 'windows')).toBe(
      'copy',
    )
  })

  it('lets a force-copy modifier win over volume defaults', () => {
    expect(resolveDropKind({ ctrlKey: true, shiftKey: false }, 'C:\\a', 'C:\\b', 'windows')).toBe(
      'copy',
    )
  })

  it('lets a force-move modifier win across volumes', () => {
    expect(resolveDropKind({ ctrlKey: false, shiftKey: true }, 'C:\\a', 'D:\\b', 'windows')).toBe(
      'move',
    )
  })
})

describe('canDropInto', () => {
  it('rejects an empty or missing payload', () => {
    expect(canDropInto(null, 'C:\\dest', 'windows')).toBe(false)
    expect(canDropInto(payload({ items: [] }), 'C:\\dest', 'windows')).toBe(false)
  })

  it('rejects dropping onto the source folder', () => {
    expect(canDropInto(payload({ sourceDir: 'C:\\root' }), 'C:\\root', 'windows')).toBe(false)
  })

  it('rejects dropping a folder into itself or its subtree', () => {
    const folder = payload({
      items: [{ id: 'd', name: 'sub', path: 'C:\\root\\sub', isDir: true, sizeBytes: null }],
    })
    expect(canDropInto(folder, 'C:\\root\\sub', 'windows')).toBe(false)
    expect(canDropInto(folder, 'C:\\root\\sub\\deep', 'windows')).toBe(false)
  })

  it('accepts a valid cross-folder drop', () => {
    expect(canDropInto(payload(), 'C:\\dest', 'windows')).toBe(true)
  })
})

describe('performDrop', () => {
  it('returns null and enqueues nothing for an invalid drop', async () => {
    const startOp = vi.fn(() => 'op-1')
    ipc.override('start_op', startOp)

    const result = await performDrop(
      payload({ sourceDir: 'C:\\root' }),
      'C:\\root',
      { ctrlKey: false, shiftKey: false },
      'windows',
    )

    expect(result).toBeNull()
    expect(startOp).not.toHaveBeenCalled()
  })

  it('enqueues a move within the same volume with mapped items', async () => {
    const startOp = vi.fn(() => 'op-7')
    ipc.override('start_op', startOp)

    const result = await performDrop(
      payload(),
      'C:\\dest',
      { ctrlKey: false, shiftKey: false },
      'windows',
    )

    expect(result).toBe('op-7')
    await waitFor(() => {
      expect(startOp).toHaveBeenCalledWith({
        kind: 'move',
        destinationDir: 'C:\\dest',
        items: [{ sourcePath: 'C:\\root\\Alpha.txt', name: 'Alpha.txt', sizeBytes: 10 }],
      })
    })
  })

  it('enqueues a copy when the force-copy modifier is held', async () => {
    const startOp = vi.fn(() => 'op-8')
    ipc.override('start_op', startOp)

    await performDrop(payload(), 'C:\\dest', { ctrlKey: true, shiftKey: false }, 'windows')

    await waitFor(() => {
      expect(startOp).toHaveBeenCalledWith(expect.objectContaining({ kind: 'copy' }))
    })
  })

  it('coerces a null size to zero for the queue engine', async () => {
    const startOp = vi.fn(() => 'op-9')
    ipc.override('start_op', startOp)

    await performDrop(
      payload({
        items: [{ id: 'd', name: 'sub', path: 'C:\\root\\sub', isDir: true, sizeBytes: null }],
      }),
      'C:\\dest',
      { ctrlKey: false, shiftKey: false },
      'windows',
    )

    await waitFor(() => {
      expect(startOp).toHaveBeenCalledWith(
        expect.objectContaining({ items: [expect.objectContaining({ sizeBytes: 0 })] }),
      )
    })
  })
})
