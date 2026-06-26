import { afterEach, vi } from 'vitest'
import { dispatch } from '@/lib/ipc/dispatch'

afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
})

describe('dispatch', () => {
  it('routes through the __TAURI_IPC__ harness when present', async () => {
    // setup.ts installs the harness (`ipc.install()`) in beforeEach.
    const response = await dispatch({ command: 'has_unfinished_ops' })
    expect(response).toBe(false)
  })

  it('routes through the Playwright mock when VITE_PLAYWRIGHT is set and no Tauri IPC', async () => {
    vi.stubGlobal('__TAURI_IPC__', undefined)
    vi.stubEnv('VITE_PLAYWRIGHT', '1')
    const response = await dispatch({ command: 'has_unfinished_ops' })
    expect(response).toBe(false)
  })

  it('falls back to the real @tauri-apps/api invoke when no harness is available', async () => {
    vi.stubGlobal('__TAURI_IPC__', undefined)
    // No VITE_PLAYWRIGHT → real-invoke path; rejects in jsdom (no Tauri runtime).
    await expect(dispatch({ command: 'has_unfinished_ops' })).rejects.toBeDefined()
  })
})
