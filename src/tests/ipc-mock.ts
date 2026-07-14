import { afterEach, vi } from 'vitest'
import type { IpcCommandMap, IpcEventMap, ListDirResponse } from '@/lib/types/ipc'
import { SESSION_PAGE_SIZE } from '@/lib/types/ipc'
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

/**
 * A monotonic session-id counter shared by the `begin_directory_session`/
 * `revise_directory_session_view` derivations below, so successive
 * navigations/view-revisions in one test never collide on `sessionId: 1` (the
 * `ListingSession` staleness guard compares baselines by value).
 */
let v2SessionIdCounter = 0
/** `sessionId -> path`, so `revise_directory_session_view` (no `path` field) can resolve the session's path. */
const sessionPathBySessionId = new Map<number, string>()

/**
 * `begin_directory_session` and `revise_directory_session_view` default to a
 * v2 response derived from whatever `list_dir` responder is active, mirroring
 * `start_list_dir`'s derivation below — the many tests that mock `list_dir`
 * transparently drive the live v2 session path too, with no separate v2
 * fixture to maintain. The caller's real `view` (sort/filter/showHidden) is
 * forwarded into the synthesized `list_dir` request so a `list_dir` responder
 * or spy that inspects its request payload still sees the actual pane view,
 * not a hardcoded default. An explicit override for either v2 command
 * (handled above) still wins, and a thrown `list_dir` responder propagates
 * here just as the real command would surface a listing failure.
 */
function deriveBeginNavigationResponse(
  paneId: string,
  tabId: string,
  path: string,
  view: IpcCommandMap['begin_directory_session']['request']['view'],
): IpcCommandMap['begin_directory_session']['response'] {
  const listing = resolveListDir({
    path,
    sortKey: view.sortKey,
    sortDirection: view.sortDirection,
    filter: view.filter,
    showHidden: view.showHidden,
    includeItemCounts: view.includeItemCounts,
  })
  v2SessionIdCounter += 1
  return {
    paneId,
    tabId,
    path: listing.path,
    baseline: {
      sessionId: v2SessionIdCounter,
      navigationRevision: v2SessionIdCounter,
      watchRevision: 0,
      viewRevision: 0,
    },
    totalRows: listing.entries.length,
    pageSize: SESSION_PAGE_SIZE,
    firstPage: { pageIndex: 0, entries: listing.entries.slice(0, SESSION_PAGE_SIZE) },
  }
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

  if (command === 'begin_directory_session') {
    const request = payload as IpcCommandMap['begin_directory_session']['request']
    const response = deriveBeginNavigationResponse(
      request.paneId,
      request.tabId,
      request.path,
      request.view,
    )
    sessionPathBySessionId.set(response.baseline.sessionId, response.path)
    return response as IpcCommandMap[CommandName]['response']
  }

  // `revise_directory_session_view` has no `path` field (a real session
  // revision reuses whatever path its session already has server-side) — the
  // mock looks the path up from the `sessionId` recorded by whichever
  // `begin_directory_session` response established it.
  if (command === 'revise_directory_session_view') {
    const request = payload as IpcCommandMap['revise_directory_session_view']['request']
    const path = sessionPathBySessionId.get(request.sessionId) ?? ''
    const response = deriveBeginNavigationResponse(
      request.paneId,
      request.tabId,
      path,
      request.view,
    )
    sessionPathBySessionId.set(response.baseline.sessionId, response.path)
    return response as IpcCommandMap[CommandName]['response']
  }

  // `get_directory_session_range` for an unmocked page defaults to an empty
  // page at whatever baseline/total the caller already has (real navigation
  // fixtures are small enough to resolve fully from `firstPage`, so this only
  // matters for a test that explicitly requests a page beyond it).
  if (command === 'get_directory_session_range') {
    const request = payload as IpcCommandMap['get_directory_session_range']['request']
    const response: IpcCommandMap['get_directory_session_range']['response'] = {
      baseline: request.baseline,
      totalRows: 0,
      page: { pageIndex: request.pageIndex, entries: [] },
    }
    return response as IpcCommandMap[CommandName]['response']
  }

  if (command === 'request_thumbnails') {
    const request = payload as IpcCommandMap['request_thumbnails']['request']
    return {
      revision: request.revision,
      acceptedCount: request.candidates.length,
    } as IpcCommandMap[CommandName]['response']
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
      | ((
          payload: IpcCommandMap[CommandName]['request'],
        ) => IpcCommandMap[CommandName]['response']),
  ) {
    overrides[command] = response as OverrideMap[CommandName]
  },
  emit<EventName extends keyof IpcEventMap>(eventName: EventName, payload: IpcEventMap[EventName]) {
    listeners[eventName]?.forEach((listener) => {
      listener(payload)
    })
  },
  listenerCount(eventName: keyof IpcEventMap) {
    return listeners[eventName]?.size ?? 0
  },
  reset() {
    for (const key of Object.keys(overrides) as (keyof OverrideMap)[]) {
      delete overrides[key]
    }

    for (const eventName of Object.keys(listeners) as (keyof EventListeners)[]) {
      listeners[eventName]?.clear()
    }

    sessionPathBySessionId.clear()
  },
}

afterEach(() => {
  ipc.reset()
})
