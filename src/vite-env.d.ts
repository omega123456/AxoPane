import type { IpcCommandMap, IpcEventMap } from '@/lib/types/ipc'

declare global {
  interface Window {
    __TAURI_IPC__?: never
  }

  var __TAURI_IPC__:
    | {
        invoke: <CommandName extends keyof IpcCommandMap>(
          command: CommandName,
          payload?: unknown,
        ) => Promise<IpcCommandMap[CommandName]['response']>
        listen: <EventName extends keyof IpcEventMap>(
          eventName: EventName,
          callback: (payload: IpcEventMap[EventName]) => void,
        ) => Promise<() => void>
      }
    | undefined
}

export {}
