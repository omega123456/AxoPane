import { beforeEach, describe, expect, it } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import {
  buildAppConfig,
  defaultAppConfig,
  hydrateAppConfig,
  isLogLevel,
} from '@/lib/app-config'
import { useConfigStore } from '@/stores/config-store'
import type { AppConfig } from '@/lib/types/ipc'

function baseConfig(overrides: Partial<AppConfig> = {}): AppConfig {
  return { ...defaultAppConfig(), ...overrides }
}

beforeEach(() => {
  ipc.install()
  useConfigStore.getState().reset()
})

describe('app-config log level', () => {
  it('defaults the log level to info', () => {
    expect(defaultAppConfig().logLevel).toBe('info')
  })

  it('recognises valid log levels and rejects others', () => {
    for (const level of ['error', 'warn', 'info', 'debug', 'trace']) {
      expect(isLogLevel(level)).toBe(true)
    }
    expect(isLogLevel('verbose')).toBe(false)
    expect(isLogLevel('')).toBe(false)
  })

  it('hydrates a valid log level into the config store', () => {
    hydrateAppConfig(baseConfig({ logLevel: 'debug' }))
    expect(useConfigStore.getState().logLevel).toBe('debug')
  })

  it('falls back to info for an invalid persisted log level', () => {
    hydrateAppConfig(baseConfig({ logLevel: 'loud' as AppConfig['logLevel'] }))
    expect(useConfigStore.getState().logLevel).toBe('info')
  })

  it('round-trips the log level through buildAppConfig', () => {
    hydrateAppConfig(baseConfig({ logLevel: 'trace' }))
    expect(buildAppConfig().logLevel).toBe('trace')
  })
})

describe('app-config date display', () => {
  it('defaults the date format, time, seconds, and relative toggles', () => {
    expect(defaultAppConfig().dateFormat).toBe('ymd')
    expect(defaultAppConfig().showTime).toBe(false)
    expect(defaultAppConfig().showSeconds).toBe(false)
    expect(defaultAppConfig().relativeDates).toBe(false)
  })

  it('hydrates date format, time, seconds, and relative toggles into the store', () => {
    hydrateAppConfig(
      baseConfig({ dateFormat: 'med', showTime: true, showSeconds: true, relativeDates: true }),
    )
    expect(useConfigStore.getState().dateFormat).toBe('med')
    expect(useConfigStore.getState().showTime).toBe(true)
    expect(useConfigStore.getState().showSeconds).toBe(true)
    expect(useConfigStore.getState().relativeDates).toBe(true)
  })

  it('falls back to the default for an invalid persisted date format', () => {
    hydrateAppConfig(baseConfig({ dateFormat: 'iso8601' as AppConfig['dateFormat'] }))
    expect(useConfigStore.getState().dateFormat).toBe('ymd')
  })

  it('migrates a legacy combined format (e.g. dmy_his) into format + showTime + showSeconds', () => {
    hydrateAppConfig(
      baseConfig({
        dateFormat: 'dmy_his' as AppConfig['dateFormat'],
        showTime: false,
        showSeconds: false,
      }),
    )
    expect(useConfigStore.getState().dateFormat).toBe('dmy')
    expect(useConfigStore.getState().showTime).toBe(true)
    expect(useConfigStore.getState().showSeconds).toBe(true)
  })

  it('treats missing time, seconds, and relative toggles as disabled', () => {
    hydrateAppConfig(
      baseConfig({
        showTime: undefined as unknown as boolean,
        showSeconds: undefined as unknown as boolean,
        relativeDates: undefined as unknown as boolean,
      }),
    )
    expect(useConfigStore.getState().showTime).toBe(false)
    expect(useConfigStore.getState().showSeconds).toBe(false)
    expect(useConfigStore.getState().relativeDates).toBe(false)
  })

  it('round-trips the date settings through buildAppConfig', () => {
    hydrateAppConfig(
      baseConfig({ dateFormat: 'dme', showTime: true, showSeconds: true, relativeDates: true }),
    )
    expect(buildAppConfig().dateFormat).toBe('dme')
    expect(buildAppConfig().showTime).toBe(true)
    expect(buildAppConfig().showSeconds).toBe(true)
    expect(buildAppConfig().relativeDates).toBe(true)
  })
})
