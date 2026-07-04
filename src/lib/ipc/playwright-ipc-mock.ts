import type { IpcCommandMap, IpcEventMap } from '@/lib/types/ipc'
import { getFixtureResponse } from '@/tests/playwright-fixtures'
import type { PlaywrightScenario } from '@/tests/playwright-fixtures/e2e'
import type { TreeChildrenByPath } from '@/tests/playwright-fixtures/tree-states'

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

function readTreeChildrenOverride(path: string, treeChildrenByPath: TreeChildrenByPath) {
  return treeChildrenByPath[path]
}

function withLiveNativeRequestId<CommandName extends keyof IpcCommandMap>(
  command: CommandName,
  payload: IpcCommandMap[CommandName]['request'],
  response: IpcCommandMap[CommandName]['response'],
) {
  if (command !== 'load_native_menu') {
    return response
  }

  return {
    ...(response as IpcCommandMap['load_native_menu']['response']),
    requestId: (payload as IpcCommandMap['load_native_menu']['request']).requestId,
  } as IpcCommandMap[CommandName]['response']
}

export async function invokePlaywrightCommand<CommandName extends keyof IpcCommandMap>(
  command: CommandName,
  payload: IpcCommandMap[CommandName]['request'],
) {
  // `start_list_dir` replaced `list_dir` as the listing entrypoint, so it
  // inherits any scenario delay/error configured for `list_dir` (loading,
  // error, and permission-denied fixtures still target `list_dir`).
  const delayMs =
    readDelay(command) || (command === 'start_list_dir' ? readDelay('list_dir') : 0)
  if (delayMs > 0) {
    await new Promise((resolve) => window.setTimeout(resolve, delayMs))
  }

  const message =
    readCommandError(command) ??
    (command === 'start_list_dir' ? readCommandError('list_dir') : undefined)
  if (message) {
    throw new Error(message)
  }

  if (command === 'list_tree_children') {
    const treeChildrenByPath = readScenario()?.treeChildrenByPath
    if (treeChildrenByPath) {
      const response = readTreeChildrenOverride(
        (payload as IpcCommandMap['list_tree_children']['request']).path,
        treeChildrenByPath,
      )
      if (response) {
        return response as IpcCommandMap[CommandName]['response']
      }
    }
  }

  // `start_list_dir` derives a "complete head" from whatever `list_dir` a
  // scenario provides, so every existing e2e fixture drives the streamed
  // listing path without declaring a separate head (mirrors the Vitest
  // harness). An explicit `start_list_dir` scenario override still wins below.
  if (command === 'start_list_dir' && !readCommandOverride('start_list_dir').found) {
    const listOverride = readCommandOverride('list_dir')
    const listing = listOverride.found ? listOverride.value : getFixtureResponse('list_dir')
    return {
      path: listing.path,
      total: listing.entries.length,
      requestId: 1,
      firstChunk: listing.entries,
      done: true,
    } as IpcCommandMap[CommandName]['response']
  }

  const override = readCommandOverride(command)
  if (override.found) {
    return withLiveNativeRequestId(command, payload, override.value)
  }
  return withLiveNativeRequestId(command, payload, getFixtureResponse(command))
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
