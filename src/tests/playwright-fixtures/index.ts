import type { IpcCommandMap } from '@/lib/types/ipc'
import { fixtures } from '../fixtures'
import { shellFixtures } from './shell'

const registry: Partial<{
  [CommandName in keyof IpcCommandMap]: IpcCommandMap[CommandName]['response']
}> = {
  ...fixtures,
  ...shellFixtures,
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
