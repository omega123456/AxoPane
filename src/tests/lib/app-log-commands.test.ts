import { afterEach, vi } from 'vitest'
import { log, logFrontend } from '@/lib/app-log-commands'

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllEnvs()
})

describe('logFrontend', () => {
  it('routes through console.info with a prefixed message and context', () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {})
    logFrontend('hello', { a: 1 })
    expect(info).toHaveBeenCalledWith('[app:info] hello', { a: 1 })
  })

  it('works without context', () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {})
    logFrontend('plain')
    expect(info).toHaveBeenCalledWith('[app:info] plain')
  })
})

describe('leveled logger', () => {
  it('emits warn and error regardless of environment', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const error = vi.spyOn(console, 'error').mockImplementation(() => {})

    vi.stubEnv('DEV', false)
    log.warn('careful', { x: 1 })
    log.error('boom')

    expect(warn).toHaveBeenCalledWith('[app:warn] careful', { x: 1 })
    expect(error).toHaveBeenCalledWith('[app:error] boom')
  })

  it('emits debug only in dev builds', () => {
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => {})

    vi.stubEnv('DEV', false)
    log.debug('quiet')
    expect(debug).not.toHaveBeenCalled()

    vi.stubEnv('DEV', true)
    log.debug('verbose', { trace: true })
    expect(debug).toHaveBeenCalledWith('[app:debug] verbose', { trace: true })
  })

  it('emits info through console.info', () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {})
    log.info('status')
    expect(info).toHaveBeenCalledWith('[app:info] status')
  })
})
