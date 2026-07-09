import { afterEach, vi } from 'vitest'
import type { IpcCommandMap, IpcEventMap, ListDirResponse } from '@/lib/types/ipc'
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

/**
 * Resolves the active `list_dir` responder (override or fixture) directly,
 * without recursing through the generic `getResponse` (which would blow up the
 * return-type union). Used to synthesize the `start_list_dir` head.
 */
function resolveListDir(payload: IpcCommandMap['list_dir']['request']): ListDirResponse {
  const override = overrides.list_dir
  if (typeof override === 'function') {
    return override(payload)
  }
  if (override !== undefined) {
    return cloneValue(override)
  }
  return cloneValue(fixtures.list_dir)
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

  // `start_list_dir` defaults to a "complete head" derived from whatever
  // `list_dir` responder is active, so the many tests that mock `list_dir`
  // transparently drive the streamed listing path without each re-mocking the
  // head. An explicit `start_list_dir` override (handled above) still wins, and
  // a thrown `list_dir` responder propagates here just as the real command
  // would surface a listing failure.
  if (command === 'start_list_dir') {
    const listing = resolveListDir(payload as IpcCommandMap['list_dir']['request'])
    const head: IpcCommandMap['start_list_dir']['response'] = {
      kind: 'head',
      path: listing.path,
      total: listing.entries.length,
      requestId: 1,
      firstChunk: listing.entries,
      done: true,
    }
    return head
  }

  // Unmocked `list_tree_children` calls default to the fixture only for the
  // exact path it describes; any other path echoes back empty children. A
  // real backend never returns another folder's children for an unrelated
  // path, and without this, every not-yet-loaded volume root would get
  // stamped with the same fixture children, producing duplicate React keys
  // once more than one root is expanded in a single test.
  if (command === 'list_tree_children') {
    const request = payload as IpcCommandMap['list_tree_children']['request']
    const fixture = fixtures.list_tree_children
    if (request.path === fixture.path) {
      return cloneValue(fixture) as IpcCommandMap[CommandName]['response']
    }
    return { path: request.path, children: [] } as IpcCommandMap[CommandName]['response']
  }

  // A command is "mocked" when it is declared in the fixtures map — even when
  // its declared response is `undefined` (void commands such as the folder-size
  // and item-count request commands). Only commands genuinely absent from the
  // fixtures throw the intentional unmocked-IPC error.
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
        // Event routing stays centralized here for every typed IPC surface,
        // including batched size/icon events and single item-count updates.
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
