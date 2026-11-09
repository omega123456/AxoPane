import type { IpcCommandMap } from '@/lib/types/ipc'
import { invokePlaywrightCommand } from './playwright-ipc-mock'

export type InvokeOptions<CommandName extends keyof IpcCommandMap> =
  IpcCommandMap[CommandName]['request'] extends undefined
    ? { command: CommandName }
    : {
        command: CommandName
        payload: IpcCommandMap[CommandName]['request']
      }

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
