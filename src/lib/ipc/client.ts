import type { IpcCommandMap, IpcEventMap } from '@/lib/types/ipc'
import { log } from '@/lib/app-log-commands'
import { listenPlaywrightEvent } from './playwright-ipc-mock'
import { dispatch, type InvokeOptions } from './dispatch'

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
