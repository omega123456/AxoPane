import { afterEach, vi } from 'vitest'
import type { IpcCommandMap, IpcEventMap } from '@/lib/types/ipc'
import { fixtures } from './fixtures'

type OverrideMap = Partial<{
  [CommandName in keyof IpcCommandMap]:
    | IpcCommandMap[CommandName]['response']
    | ((payload: IpcCommandMap[CommandName]['request']) => IpcCommandMap[CommandName]['response'])
}>

type EventListeners = Record<string, Set<(payload: unknown) => void> | undefined>

const overrides: OverrideMap = {}
const listeners: EventListeners = {}

function cloneValue<T>(value: T): T {
  return structuredClone(value)
}

function getResponse<CommandName extends keyof IpcCommandMap>(
  command: CommandName,
  payload: IpcCommandMap[CommandName]['request'],
) {
  const override = overrides[command]

  if (typeof override === 'function') {
    return override(payload)
  }

  if (override !== undefined) {
    return cloneValue(override)
  }

  // A command is "mocked" when it is declared in the fixtures map — even when
  // its declared response is `undefined` (void commands such as the folder-size
  // requests). Only commands genuinely absent from the fixtures throw the
  // intentional unmocked-IPC error.
  if (!Object.prototype.hasOwnProperty.call(fixtures, command)) {
    throw new Error(`[vitest] Unmocked Tauri IPC command: ${String(command)}`)
  }

  const fixture = fixtures[command]
  if (fixture === undefined) {
    return undefined as IpcCommandMap[CommandName]['response']
  }

  return cloneValue(fixture)
}

export const ipc = {
  install() {
    vi.stubGlobal('__TAURI_IPC__', {
      invoke: (command: keyof IpcCommandMap, payload?: unknown) =>
        Promise.resolve(
          getResponse(
            command,
            ((payload as { payload?: unknown } | undefined)?.payload ?? payload) as never,
          ),
        ),
      listen: (eventName: keyof IpcEventMap, callback: (payload: unknown) => void) => {
        const eventListeners = listeners[eventName] ?? new Set()
        eventListeners.add(callback)
        listeners[eventName] = eventListeners
        return Promise.resolve(() => {
          eventListeners.delete(callback)
        })
      },
    })
  },
  override<CommandName extends keyof IpcCommandMap>(
    command: CommandName,
    response:
      | IpcCommandMap[CommandName]['response']
      | ((payload: IpcCommandMap[CommandName]['request']) => IpcCommandMap[CommandName]['response']),
  ) {
    overrides[command] = response as OverrideMap[CommandName]
  },
  emit<EventName extends keyof IpcEventMap>(eventName: EventName, payload: IpcEventMap[EventName]) {
    listeners[eventName]?.forEach((listener) => {
      listener(payload)
    })
  },
  reset() {
    for (const key of Object.keys(overrides) as (keyof OverrideMap)[]) {
      delete overrides[key]
    }

    for (const eventName of Object.keys(listeners) as (keyof EventListeners)[]) {
      listeners[eventName]?.clear()
    }
  },
}

afterEach(() => {
  ipc.reset()
})
