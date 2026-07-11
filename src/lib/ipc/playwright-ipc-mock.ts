import type { IpcCommandMap, IpcEventMap } from '@/lib/types/ipc'
import { SESSION_PAGE_SIZE } from '@/lib/types/ipc'
import { TRASH_PATH } from '@/lib/trash'
import { getFixtureResponse } from '@/tests/playwright-fixtures'
import type { PlaywrightScenario } from '@/tests/playwright-fixtures/e2e'
import type { TreeChildrenByPath } from '@/tests/playwright-fixtures/tree-states'

/**
 * A monotonic session-id counter for the `begin_directory_session`/
 * `revise_directory_session_view` derivation below, so successive
 * navigations/view-revisions within one spec never collide on `sessionId: 1`.
 */
let v2SessionIdCounter = 0

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

function listVolumesFixture() {
  const override = readCommandOverride('list_volumes')
  return override.found ? override.value : getFixtureResponse('list_volumes')
}

function isVolumeRoot(path: string) {
  return listVolumesFixture().some((volume) => volume.mountRoot.toLowerCase() === path.toLowerCase())
}

function loadSessionFixture() {
  const override = readCommandOverride('load_session')
  return override.found ? override.value : getFixtureResponse('load_session')
}

function rootForPath(path: string) {
  const normalized = path.toLowerCase()
  return listVolumesFixture()
    .filter((volume) => {
      const root = volume.mountRoot.toLowerCase()
      return normalized === root || normalized.startsWith(root)
    })
    .sort((left, right) => right.mountRoot.length - left.mountRoot.length)[0]?.mountRoot
}

function defaultTreeFixtureTarget() {
  const session = loadSessionFixture()
  const preferred =
    session.activePane === 'right'
      ? [session.rightPath, session.leftPath]
      : [session.leftPath, session.rightPath]

  for (const path of preferred) {
    if (!path || path === TRASH_PATH) {
      continue
    }
    const root = rootForPath(path)
    if (root) {
      return { root, path }
    }
  }

  return undefined
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
  // The live directory-session entrypoint inherits any scenario delay/error configured for `list_dir`
  // (loading, error, and permission-denied fixtures still target `list_dir`).
  const inheritsListDirTiming = command === 'begin_directory_session'
  const delayMs = readDelay(command) || (inheritsListDirTiming ? readDelay('list_dir') : 0)
  if (delayMs > 0) {
    await new Promise((resolve) => window.setTimeout(resolve, delayMs))
  }

  const message =
    readCommandError(command) ?? (inheritsListDirTiming ? readCommandError('list_dir') : undefined)
  if (message) {
    throw new Error(message)
  }

  if (command === 'list_tree_children') {
    const path = (payload as IpcCommandMap['list_tree_children']['request']).path
    const treeChildrenByPath = readScenario()?.treeChildrenByPath
    if (treeChildrenByPath) {
      const response = readTreeChildrenOverride(path, treeChildrenByPath)
      if (response) {
        return response as IpcCommandMap[CommandName]['response']
      }
    }

    const defaultTreeTarget = defaultTreeFixtureTarget()
    if (
      isVolumeRoot(path) &&
      defaultTreeTarget?.root.toLowerCase() === path.toLowerCase() &&
      defaultTreeTarget.path.toLowerCase() !== defaultTreeTarget.root.toLowerCase()
    ) {
      const fixture = getFixtureResponse('list_tree_children')
      return {
        path,
        children: fixture.children,
      } as IpcCommandMap[CommandName]['response']
    }

    return {
      path,
      children: [],
    } as IpcCommandMap[CommandName]['response']
  }

  // `begin_directory_session` / `revise_directory_session_view` derive a v2
  // response from whatever `list_dir` a scenario provides, mirroring
  // the normal listing fixture, so the live app's v2 navigation path is driven by
  // the same per-spec fixtures without a separate v2 fixture to maintain. An
  // explicit scenario override for either v2 command still wins below.
  if (
    (command === 'begin_directory_session' || command === 'revise_directory_session_view') &&
    !readCommandOverride(command).found
  ) {
    const listOverride = readCommandOverride('list_dir')
    const listing = listOverride.found ? listOverride.value : getFixtureResponse('list_dir')
    const beginPayload = payload as IpcCommandMap['begin_directory_session']['request']
    v2SessionIdCounter += 1
    return {
      paneId: beginPayload.paneId,
      tabId: beginPayload.tabId,
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
    } as IpcCommandMap[CommandName]['response']
  }

  // `get_directory_session_range` for an unmocked page defaults to an empty
  // page at whatever baseline the caller already has; every e2e fixture
  // listing is small enough to resolve fully from `firstPage` above.
  if (command === 'get_directory_session_range' && !readCommandOverride(command).found) {
    const request = payload as IpcCommandMap['get_directory_session_range']['request']
    return {
      baseline: request.baseline,
      totalRows: 0,
      page: { pageIndex: request.pageIndex, entries: [] },
    } as IpcCommandMap[CommandName]['response']
  }

  // Explicit Items sorting reuses the shared fixture registry by default so a
  // scenario can exercise the pending visual state with only a delay override.
  if (command === 'sort_active_items' && !readCommandOverride('sort_active_items').found) {
    return getFixtureResponse('sort_active_items') as IpcCommandMap[CommandName]['response']
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
