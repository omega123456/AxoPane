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
