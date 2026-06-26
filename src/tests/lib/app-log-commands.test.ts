import { afterEach, vi } from 'vitest'
import { log, logFrontend } from '@/lib/app-log-commands'
import { ipc } from '@/tests/ipc-mock'
import type { LogFrontendRequest } from '@/lib/types/ipc'

/**
 * Capture every `log_frontend` payload routed through the shared IPC harness.
 * The harness invokes overrides synchronously, so payloads are available right
 * after the (fire-and-forget) logger call.
 */
function captureLogCalls() {
  const calls: LogFrontendRequest[] = []
  ipc.override('log_frontend', (payload) => {
    calls.push(payload as LogFrontendRequest)
    return undefined
  })
  return calls
}

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllEnvs()
})

describe('logFrontend', () => {
  it('routes through the log_frontend IPC command with serialized context', () => {
    const calls = captureLogCalls()
    logFrontend('hello', { a: 1 })
    expect(calls).toEqual([
      { level: 'info', message: 'hello', category: 'frontend', details: '{"a":1}' },
    ])
  })

  it('omits details when no context is supplied', () => {
    const calls = captureLogCalls()
    logFrontend('plain')
    expect(calls).toEqual([
      { level: 'info', message: 'plain', category: 'frontend', details: undefined },
    ])
  })

  it('falls back to String(context) when context is not serializable', () => {
    const calls = captureLogCalls()
    const circular: Record<string, unknown> = {}
    circular.self = circular
    logFrontend('cyclic', circular)
    expect(calls).toHaveLength(1)
    expect(calls[0].details).toBe(String(circular))
  })
})

describe('leveled logger', () => {
  it('emits warn and error regardless of environment', () => {
    const calls = captureLogCalls()
    vi.stubEnv('DEV', false)
    log.warn('careful', { x: 1 })
    log.error('boom')
    expect(calls).toEqual([
      { level: 'warn', message: 'careful', category: 'frontend', details: '{"x":1}' },
      { level: 'error', message: 'boom', category: 'frontend', details: undefined },
    ])
  })

  it('emits debug only in dev builds', () => {
    const calls = captureLogCalls()

    vi.stubEnv('DEV', false)
    log.debug('quiet')
    expect(calls).toHaveLength(0)

    vi.stubEnv('DEV', true)
    log.debug('verbose', { trace: true })
    expect(calls).toEqual([
      { level: 'debug', message: 'verbose', category: 'frontend', details: '{"trace":true}' },
    ])
  })

  it('emits info through the IPC sink', () => {
    const calls = captureLogCalls()
    log.info('status')
    expect(calls).toEqual([
      { level: 'info', message: 'status', category: 'frontend', details: undefined },
    ])
  })

  it('falls back to console when the IPC sink rejects', async () => {
    const errorSink = vi.spyOn(console, 'error').mockImplementation(() => {})
    // Return a rejected promise (as real Tauri would) rather than throwing
    // synchronously, so the rejection flows into the logger's `.catch`.
    ipc.override('log_frontend', () => Promise.reject(new Error('ipc down')) as never)

    log.error('boom', { code: 7 })

    await vi.waitFor(() => expect(errorSink).toHaveBeenCalledTimes(1))
    expect(errorSink).toHaveBeenCalledWith('[app:error] boom', { code: 7 }, expect.any(Error))
  })
})
