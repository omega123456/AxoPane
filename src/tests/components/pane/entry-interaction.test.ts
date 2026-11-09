import type { DragEvent, MouseEvent } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { entryInteractionProps } from '@/components/pane/entry-interaction'
import type { FileRowActions } from '@/components/pane/FileRow'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { MIDDLE_MOUSE_BUTTON, PRIMARY_MOUSE_BUTTON } from '@/lib/mouse-buttons'

const FOLDER: DirectoryEntry = {
  id: 'folder-id',
  name: 'Folder',
  path: '/root/Folder',
  isDir: true,
  iconDataUrl: null,
  sizeBytes: null,
  itemCount: null,
  typeLabel: 'Folder',
  modifiedAt: null,
  createdAt: null,
  attributes: [],
  isHidden: false,
  isSystem: false,
}

function stubActions(): FileRowActions {
  return {
    onPointerDown: vi.fn(),
    onActivate: vi.fn(),
    onClick: vi.fn(),
    onContextMenu: vi.fn(),
    onMiddleClick: vi.fn(),
    onDragStart: vi.fn(),
    onDragEnd: vi.fn(),
    onDragEnter: vi.fn(),
    onDragOver: vi.fn(),
    onDragLeave: vi.fn(),
    onDrop: vi.fn(),
    onRenameChange: vi.fn(),
    onRenameSubmit: vi.fn(),
    onRenameCancel: vi.fn(),
    onRenameBlur: vi.fn(),
  }
}

function mouseEvent(button: number) {
  return {
    button,
    detail: 2,
    timeStamp: 42,
    preventDefault: vi.fn(),
  } as unknown as MouseEvent<HTMLDivElement> & { preventDefault: ReturnType<typeof vi.fn> }
}

describe('entryInteractionProps', () => {
  it('binds the entry id into every drag and pointer handler', () => {
    const actions = stubActions()
    const props = entryInteractionProps(FOLDER, actions)
    const dragEvent = {} as DragEvent<HTMLDivElement>
    const clickEvent = mouseEvent(PRIMARY_MOUSE_BUTTON)

    props.onDragStart(dragEvent)
    props.onDragEnter(dragEvent)
    props.onDragOver(dragEvent)
    props.onDragLeave()
    props.onDrop(dragEvent)
    props.onDoubleClick(clickEvent)
    props.onClick(clickEvent)
    props.onContextMenu(clickEvent)

    expect(actions.onDragStart).toHaveBeenCalledWith('folder-id', dragEvent)
    expect(actions.onDragEnter).toHaveBeenCalledWith('folder-id', dragEvent)
    expect(actions.onDragOver).toHaveBeenCalledWith('folder-id', dragEvent)
    expect(actions.onDragLeave).toHaveBeenCalledWith('folder-id')
    expect(actions.onDrop).toHaveBeenCalledWith('folder-id', dragEvent)
    expect(actions.onActivate).toHaveBeenCalledWith('folder-id', 2, 42)
    expect(actions.onClick).toHaveBeenCalledWith('folder-id', clickEvent)
    expect(actions.onContextMenu).toHaveBeenCalledWith('folder-id', clickEvent)
    expect(props.onDragEnd).toBe(actions.onDragEnd)
    expect(props.onMouseDown).toBe(actions.onPointerDown)
  })

  it('opens a folder on middle click and ignores other buttons', () => {
    const actions = stubActions()
    const props = entryInteractionProps(FOLDER, actions)
    const primaryClick = mouseEvent(PRIMARY_MOUSE_BUTTON)
    const middleClick = mouseEvent(MIDDLE_MOUSE_BUTTON)

    props.onAuxClick(primaryClick)
    expect(actions.onMiddleClick).not.toHaveBeenCalled()
    expect(primaryClick.preventDefault).not.toHaveBeenCalled()

    props.onAuxClick(middleClick)
    expect(actions.onMiddleClick).toHaveBeenCalledWith('folder-id')
    expect(middleClick.preventDefault).toHaveBeenCalledOnce()
  })
})
