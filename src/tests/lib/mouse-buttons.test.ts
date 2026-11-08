import {
  isMiddleButton,
  isMiddleClickOpen,
  MIDDLE_MOUSE_BUTTON,
  PRIMARY_MOUSE_BUTTON,
} from '@/lib/mouse-buttons'

const middleClick = { button: MIDDLE_MOUSE_BUTTON }
const primaryClick = { button: PRIMARY_MOUSE_BUTTON }

describe('isMiddleButton', () => {
  it('matches only the middle button', () => {
    expect(isMiddleButton(middleClick)).toBe(true)
    expect(isMiddleButton(primaryClick)).toBe(false)
  })
})

describe('isMiddleClickOpen', () => {
  it('opens a folder on a middle click', () => {
    expect(isMiddleClickOpen(middleClick, { isDir: true })).toBe(true)
  })

  it('ignores files, trashed folders and other buttons', () => {
    expect(isMiddleClickOpen(middleClick, { isDir: false })).toBe(false)
    expect(isMiddleClickOpen(middleClick, { isDir: true, trashId: 'trash-1' })).toBe(false)
    expect(isMiddleClickOpen(primaryClick, { isDir: true })).toBe(false)
  })
})
