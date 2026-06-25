import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { installCloseGuard, type GuardableWindow } from '@/lib/close-guard'

type CloseHandler = (event: { preventDefault: () => void }) => void | Promise<void>

function fakeWindow() {
  const destroy = vi.fn(() => Promise.resolve())
  let handler: CloseHandler | undefined
  const win: GuardableWindow = {
    onCloseRequested: (next) => {
      handler = next
      return Promise.resolve(() => {})
    },
    destroy,
  }
  return {
    win,
    destroy,
    async fireClose() {
      const preventDefault = vi.fn()
      await handler?.({ preventDefault })
      return preventDefault
    },
  }
}

beforeEach(() => {
  ipc.install()
})

describe('installCloseGuard', () => {
  it('does nothing without a guardable window', async () => {
    const unlisten = await installCloseGuard(async () => null)
    expect(typeof unlisten).toBe('function')
    unlisten()
  })

  it('returns a no-op guard when there is no Tauri runtime', async () => {
    vi.stubGlobal('__TAURI_IPC__', undefined)
    const unlisten = await installCloseGuard()
    expect(typeof unlisten).toBe('function')
    unlisten()
    vi.unstubAllGlobals()
  })

  it('uses window.confirm by default and proceeds when accepted', async () => {
    ipc.override('has_unfinished_ops', true)
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true)
    const { win, fireClose, destroy } = fakeWindow()
    await installCloseGuard(async () => win)

    const preventDefault = await fireClose()
    expect(confirmSpy).toHaveBeenCalled()
    expect(preventDefault).not.toHaveBeenCalled()
    expect(destroy).toHaveBeenCalled()
    confirmSpy.mockRestore()
  })

  it('allows close immediately when no work is unfinished', async () => {
    ipc.override('has_unfinished_ops', false)
    const { win, fireClose, destroy } = fakeWindow()
    await installCloseGuard(async () => win)

    const preventDefault = await fireClose()
    expect(preventDefault).not.toHaveBeenCalled()
    expect(destroy).not.toHaveBeenCalled()
  })

  it('blocks close when work is unfinished and the user declines', async () => {
    ipc.override('has_unfinished_ops', true)
    const { win, fireClose, destroy } = fakeWindow()
    await installCloseGuard(
      async () => win,
      () => false,
    )

    const preventDefault = await fireClose()
    expect(preventDefault).toHaveBeenCalled()
    expect(destroy).not.toHaveBeenCalled()
  })

  it('proceeds to destroy when the user confirms closing', async () => {
    ipc.override('has_unfinished_ops', true)
    const { win, fireClose, destroy } = fakeWindow()
    await installCloseGuard(
      async () => win,
      () => true,
    )

    const preventDefault = await fireClose()
    expect(preventDefault).not.toHaveBeenCalled()
    expect(destroy).toHaveBeenCalled()
  })
})
