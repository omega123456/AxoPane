import type { IpcCommandMap, IpcEventMap } from '@/lib/types/ipc'
import { getFixtureResponse } from '@/tests/playwright-fixtures'
import type { PlaywrightScenario } from '@/tests/playwright-fixtures/e2e'

type ListenerMap = {
  [eventName: string]: Set<(payload: unknown) => void> | undefined
}

const listeners: ListenerMap = {}

function readScenario() {
  return (globalThis as { __PLAYWRIGHT_IPC_SCENARIO__?: PlaywrightScenario }).__PLAYWRIGHT_IPC_SCENARIO__
}

/**
 * Per-test fixture overrides. A spec can set
 * `window.__PLAYWRIGHT_IPC_OVERRIDES__ = { command: response }` (via
 * `page.addInitScript`) to drive a specific UI state without embedding any
 * domain data inside this router — the override values still come from the
 * spec's chosen fixture.
 */
function readCommandOverride<CommandName extends keyof IpcCommandMap>(command: CommandName) {
  const overrides = readScenario()?.commands
  if (overrides && command in overrides) {
    return { found: true, value: overrides[command] as IpcCommandMap[CommandName]['response'] }
  }
  return { found: false, value: undefined as IpcCommandMap[CommandName]['response'] }
}

function readCommandError<CommandName extends keyof IpcCommandMap>(command: CommandName) {
  return readScenario()?.commandErrors?.[command]
}

function readDelay<CommandName extends keyof IpcCommandMap>(command: CommandName) {
  return readScenario()?.delaysMs?.[command] ?? 0
}

export async function invokePlaywrightCommand<CommandName extends keyof IpcCommandMap>(
  command: CommandName,
  _payload: IpcCommandMap[CommandName]['request'],
) {
  void _payload
  const delayMs = readDelay(command)
  if (delayMs > 0) {
    await new Promise((resolve) => window.setTimeout(resolve, delayMs))
  }

  const message = readCommandError(command)
  if (message) {
    throw new Error(message)
  }

  const override = readCommandOverride(command)
  if (override.found) {
    return override.value
  }
  return getFixtureResponse(command)
}

export async function listenPlaywrightEvent<EventName extends keyof IpcEventMap>(
  eventName: EventName,
  handler: (payload: IpcEventMap[EventName]) => void,
) {
  const handlers = listeners[eventName] ?? new Set()
  handlers.add(handler as (payload: unknown) => void)
  listeners[eventName] = handlers

  const scriptedEvents = readScenario()?.events?.[eventName]
  if (scriptedEvents) {
    for (const payload of scriptedEvents) {
      window.setTimeout(() => {
        const currentHandlers = listeners[eventName]
        if (!currentHandlers) {
          return
        }
        for (const currentHandler of currentHandlers) {
          currentHandler(payload)
        }
      }, 0)
    }
  }

  return () => {
    handlers.delete(handler as (payload: unknown) => void)
  }
}
