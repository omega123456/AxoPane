import { vi } from 'vitest'
import { ipc } from '../ipc-mock'
import { applyWindowBackdrop, syncNativeTheme } from '@/lib/window-commands'

afterEach(() => {
  document.documentElement.classList.remove('mica')
})

describe('window-commands', () => {
  it('marks the document for Mica only when Rust applied it', async () => {
    ipc.override('get_window_backdrop', 'mica')
    await applyWindowBackdrop()
    expect(document.documentElement).toHaveClass('mica')

    ipc.override('get_window_backdrop', 'solid')
    await applyWindowBackdrop()
    expect(document.documentElement).not.toHaveClass('mica')
  })

  it('keeps the solid base and logs when the backdrop lookup fails', async () => {
    const logged = vi.fn()
    ipc.override('log_frontend', logged)
    ipc.override('get_window_backdrop', () => {
      throw new Error('no window')
    })

    await applyWindowBackdrop()

    expect(document.documentElement).not.toHaveClass('mica')
    expect(logged).toHaveBeenCalledWith(
      expect.objectContaining({ message: 'get_window_backdrop failed' }),
    )
  })

  it('logs a title-bar theme that could not be applied', async () => {
    const logged = vi.fn()
    ipc.override('log_frontend', logged)
    ipc.override('set_window_theme', () => {
      throw new Error('no window')
    })

    await syncNativeTheme('dark')

    expect(logged).toHaveBeenCalledWith(
      expect.objectContaining({ message: 'set_window_theme failed' }),
    )
  })
})
