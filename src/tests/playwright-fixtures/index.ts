import type { IpcCommandMap } from '@/lib/types/ipc'
import { fixtures } from '../fixtures'
import { contextMenuFixtures } from './context-menu'
import { shellFixtures } from './shell'

const registry: Partial<{
  [CommandName in keyof IpcCommandMap]: IpcCommandMap[CommandName]['response']
}> = {
  ...fixtures,
  ...shellFixtures,
  load_native_menu: contextMenuFixtures.emptyNativeExtras,
}

export function getFixtureResponse<CommandName extends keyof IpcCommandMap>(command: CommandName) {
  if (!(command in registry)) {
    throw new Error(`[playwright] Unmocked Tauri IPC command: ${String(command)}`)
  }

  const response = registry[command]
  if (response === undefined) {
    return undefined as IpcCommandMap[CommandName]['response']
  }

  return structuredClone(response)
}
