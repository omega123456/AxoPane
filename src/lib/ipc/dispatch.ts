import type { IpcCommandMap } from '@/lib/types/ipc'
import { invokePlaywrightCommand } from './playwright-ipc-mock'

/**
 * Shape of an IPC call: just the command for request-less commands, or the
 * command plus its typed payload otherwise.
 */
export type InvokeOptions<CommandName extends keyof IpcCommandMap> =
  IpcCommandMap[CommandName]['request'] extends undefined
    ? { command: CommandName }
    : {
        command: CommandName
        payload: IpcCommandMap[CommandName]['request']
      }

/**
 * Low-level IPC dispatch that picks the right transport for the current
 * environment (real Tauri, the Vitest `__TAURI_IPC__` harness, the Playwright
 * mock, or a lazily-imported real `invoke`). Deliberately free of logging so the
 * frontend logger (`app-log-commands.ts`) can route `log_frontend` through it
 * without recursing back into itself.
 */
export function dispatch<CommandName extends keyof IpcCommandMap>(
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
