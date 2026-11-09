import type { IpcCommandMap, IpcEventMap } from '@/lib/types/ipc'
import { SESSION_PAGE_SIZE } from '@/lib/types/ipc'
import { TRASH_PATH } from '@/lib/trash'
import { getFixtureResponse, resolveThumbnailFixtureBatch } from '@/tests/playwright-fixtures'
import type { PlaywrightScenario } from '@/tests/playwright-fixtures/e2e'
import type { TreeChildrenByPath } from '@/tests/playwright-fixtures/tree-states'

let v2SessionIdCounter = 0

type ListenerMap = {
  [eventName: string]: Set<(payload: unknown) => void> | undefined
}

const listeners: ListenerMap = {}
const cancelledThumbnailScopes = new Set<string>()

function thumbnailScopeKey(
  payload: Pick<
    IpcCommandMap['request_thumbnails']['request'],
    'paneId' | 'tabId' | 'path' | 'generation'
  >,
) {
  return `${payload.paneId}:${payload.tabId}:${payload.path}:${payload.generation}`
}

function emitPlaywrightEvent<EventName extends keyof IpcEventMap>(
  eventName: EventName,
  payload: IpcEventMap[EventName],
) {
  for (const handler of listeners[eventName] ?? []) {
    handler(payload)
  }
}

function readScenario() {
  return (globalThis as { __PLAYWRIGHT_IPC_SCENARIO__?: PlaywrightScenario })
    .__PLAYWRIGHT_IPC_SCENARIO__
}

function readCommandOverride<CommandName extends keyof IpcCommandMap>(command: CommandName) {
  const overrides = readScenario()?.commands
  if (!overrides) {
    return { found: false, value: undefined as IpcCommandMap[CommandName]['response'] }
  }
  if (command in overrides) {
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

function overriddenOrFixture<CommandName extends keyof IpcCommandMap>(command: CommandName) {
  const override = readCommandOverride(command)
  return override.found ? override.value : getFixtureResponse(command)
}

function listVolumesFixture() {
  return overriddenOrFixture('list_volumes')
}

function isVolumeRoot(path: string) {
  return listVolumesFixture().some(
    (volume) => volume.mountRoot.toLowerCase() === path.toLowerCase(),
  )
}

function loadSessionFixture() {
  return overriddenOrFixture('load_session')
}

function rootForPath(path: string) {
  const normalized = path.toLowerCase()
  return listVolumesFixture()
    .filter((volume) => normalized.startsWith(volume.mountRoot.toLowerCase()))
    .sort((left, right) => right.mountRoot.length - left.mountRoot.length)[0]?.mountRoot
}

function defaultTreeFixtureTarget() {
  const session = loadSessionFixture()
  const preferred =
    session.activePane === 'right'
      ? [session.rightPath, session.leftPath]
      : [session.leftPath, session.rightPath]

  for (const path of preferred) {
    if (!path) {
      continue
    }
    if (path === TRASH_PATH) {
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

type CommandRequest<CommandName extends keyof IpcCommandMap> = IpcCommandMap[CommandName]['request']
type CommandResponse<CommandName extends keyof IpcCommandMap> =
  IpcCommandMap[CommandName]['response']
type CommandRoutes = Partial<{
  [CommandName in keyof IpcCommandMap]: (
    payload: CommandRequest<CommandName>,
  ) => CommandResponse<CommandName>
}>
type CommandRoute<CommandName extends keyof IpcCommandMap> =
  | ((payload: CommandRequest<CommandName>) => CommandResponse<CommandName>)
  | undefined

function isDefaultTreeRoot(path: string) {
  const defaultTreeTarget = defaultTreeFixtureTarget()
  if (!defaultTreeTarget) {
    return false
  }
  if (!isVolumeRoot(path)) {
    return false
  }
  const root = defaultTreeTarget.root.toLowerCase()
  const isTargetRoot = root === path.toLowerCase()
  const targetLiesBelowRoot = defaultTreeTarget.path.toLowerCase() !== root
  return isTargetRoot && targetLiesBelowRoot
}

function treeChildrenResponse({ path }: CommandRequest<'list_tree_children'>) {
  const scenarioChildren = readScenario()?.treeChildrenByPath
  const override = scenarioChildren ? readTreeChildrenOverride(path, scenarioChildren) : undefined
  if (override) {
    return override
  }
  const children = isDefaultTreeRoot(path) ? getFixtureResponse('list_tree_children').children : []
  return { path, children }
}

function directorySessionResponse(
  payload: Pick<CommandRequest<'begin_directory_session'>, 'paneId' | 'tabId'>,
): CommandResponse<'begin_directory_session'> {
  const listing = overriddenOrFixture('list_dir')
  v2SessionIdCounter += 1
  return {
    paneId: payload.paneId,
    tabId: payload.tabId,
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

function emptySessionRange(request: CommandRequest<'get_directory_session_range'>) {
  return {
    baseline: request.baseline,
    totalRows: 0,
    page: { pageIndex: request.pageIndex, entries: [] },
  }
}

function requestThumbnails(request: CommandRequest<'request_thumbnails'>) {
  const fixture = readScenario()?.thumbnailFixture
  const scopeKey = thumbnailScopeKey(request)
  cancelledThumbnailScopes.delete(scopeKey)
  if (fixture) {
    const batch = resolveThumbnailFixtureBatch(request, fixture)
    window.setTimeout(() => {
      if (!cancelledThumbnailScopes.has(scopeKey)) {
        emitPlaywrightEvent('thumbnail://state', batch)
      }
    }, 0)
  }
  return { revision: request.revision, acceptedCount: request.candidates.length }
}

const responsesIgnoringOverrides: CommandRoutes = {
  list_tree_children: treeChildrenResponse,
  cancel_thumbnails: (request) => {
    cancelledThumbnailScopes.add(thumbnailScopeKey(request))
  },
  request_thumbnails: requestThumbnails,
}

const responsesWhenNotOverridden: CommandRoutes = {
  begin_directory_session: directorySessionResponse,
  revise_directory_session_view: directorySessionResponse,
  get_directory_session_range: emptySessionRange,
  sort_active_items: () => getFixtureResponse('sort_active_items'),
}

function inheritsListDirTiming(command: keyof IpcCommandMap) {
  return command === 'begin_directory_session'
}

function scenarioDelayMs(command: keyof IpcCommandMap) {
  const fallback = inheritsListDirTiming(command) ? readDelay('list_dir') : 0
  return readDelay(command) || fallback
}

function scenarioFailure(command: keyof IpcCommandMap) {
  const fallback = inheritsListDirTiming(command) ? readCommandError('list_dir') : undefined
  return readCommandError(command) ?? fallback
}

function throwScenarioFailure(command: keyof IpcCommandMap) {
  const failure = scenarioFailure(command)
  if (typeof failure === 'string') {
    throw new Error(failure)
  }
  if (failure) {
    throw structuredClone(failure)
  }
}

function lookupRoute<CommandName extends keyof IpcCommandMap>(
  routes: CommandRoutes,
  command: CommandName,
) {
  return routes[command] as CommandRoute<CommandName>
}

export async function invokePlaywrightCommand<CommandName extends keyof IpcCommandMap>(
  command: CommandName,
  payload: CommandRequest<CommandName>,
): Promise<CommandResponse<CommandName>> {
  const delayMs = scenarioDelayMs(command)
  if (delayMs > 0) {
    await new Promise((resolve) => window.setTimeout(resolve, delayMs))
  }
  throwScenarioFailure(command)

  const routed = lookupRoute(responsesIgnoringOverrides, command)
  if (routed) {
    return routed(payload)
  }
  const override = readCommandOverride(command)
  if (override.found) {
    return withLiveNativeRequestId(command, payload, override.value)
  }
  const unmockedDefault = lookupRoute(responsesWhenNotOverridden, command)
  if (unmockedDefault) {
    return unmockedDefault(payload)
  }
  return withLiveNativeRequestId(command, payload, getFixtureResponse(command))
}

declare global {
  interface Window {
    __PLAYWRIGHT_IPC_EMIT__?: <EventName extends keyof IpcEventMap>(
      eventName: EventName,
      payload: IpcEventMap[EventName],
    ) => void
  }
}

export async function listenPlaywrightEvent<EventName extends keyof IpcEventMap>(
  eventName: EventName,
  handler: (payload: IpcEventMap[EventName]) => void,
) {
  const handlers = listeners[eventName] ?? new Set()
  handlers.add(handler as (payload: unknown) => void)
  listeners[eventName] = handlers
  window.__PLAYWRIGHT_IPC_EMIT__ = emitPlaywrightEvent

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
