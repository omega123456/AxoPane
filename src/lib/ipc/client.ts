import type { IpcCommandMap, IpcEventMap } from '@/lib/types/ipc'
import { log } from '@/lib/app-log-commands'
import { invokePlaywrightCommand, listenPlaywrightEvent } from './playwright-ipc-mock'

type InvokeOptions<CommandName extends keyof IpcCommandMap> =
  IpcCommandMap[CommandName]['request'] extends undefined
    ? { command: CommandName }
    : {
        command: CommandName
        payload: IpcCommandMap[CommandName]['request']
      }

function dispatch<CommandName extends keyof IpcCommandMap>(
  options: InvokeOptions<CommandName>,
): Promise<IpcCommandMap[CommandName]['response']> {
  const tauriIpc = globalThis.__TAURI_IPC__

  if (tauriIpc) {
    return tauriIpc.invoke(
      options.command,
      'payload' in options ? { payload: options.payload } : undefined,
    ) as Promise<IpcCommandMap[CommandName]['response']>
  }

  if (import.meta.env.VITE_PLAYWRIGHT) {
    return invokePlaywrightCommand(
      options.command,
      'payload' in options ? options.payload : undefined,
    ) as Promise<IpcCommandMap[CommandName]['response']>
  }

  return import('@tauri-apps/api/core').then(({ invoke }) =>
    invoke<IpcCommandMap[CommandName]['response']>(
      options.command,
      'payload' in options ? { payload: options.payload } : undefined,
    ),
  )
}

export async function invokeCommand<CommandName extends keyof IpcCommandMap>(
  options: InvokeOptions<CommandName>,
): Promise<IpcCommandMap[CommandName]['response']> {
  const payload = 'payload' in options ? options.payload : undefined
  log.debug(`ipc → ${String(options.command)}`, { payload })

  const startedAt = performance.now()
  try {
    const response = await dispatch(options)
    const durationMs = Math.round(performance.now() - startedAt)
    log.debug(`ipc ✓ ${String(options.command)}`, { durationMs })
    return response
  } catch (error) {
    const durationMs = Math.round(performance.now() - startedAt)
    log.error(`ipc ✗ ${String(options.command)}`, { durationMs, error })
    throw error
  }
}

export async function subscribeToEvent<EventName extends keyof IpcEventMap>(
  eventName: EventName,
  handler: (payload: IpcEventMap[EventName]) => void,
) {
  log.debug(`ipc subscribe → ${String(eventName)}`)
  const tauriIpc = globalThis.__TAURI_IPC__

  if (tauriIpc) {
    return tauriIpc.listen(eventName, handler as (payload: unknown) => void)
  }

  if (import.meta.env.VITE_PLAYWRIGHT) {
    return listenPlaywrightEvent(eventName, handler)
  }

  const { listen } = await import('@tauri-apps/api/event')
  const unlisten = await listen<IpcEventMap[EventName]>(eventName, (event) => {
    handler(event.payload)
  })

  return () => {
    unlisten()
  }
}
