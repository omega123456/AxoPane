import { log } from '@/lib/app-log-commands'
import { ejectVolume } from '@/lib/ipc/commands'
import type { EjectVolumeRequest, MenuActionStatus } from '@/lib/types/ipc'

export async function runEjectVolume({ mountRoot }: EjectVolumeRequest): Promise<MenuActionStatus> {
  try {
    const response = await ejectVolume({ mountRoot })
    if (!response.handled) {
      log.info('eject command unavailable', {
        mountRoot,
        message: response.message ?? null,
      })
      return response
    }

    log.info('eject command completed', { mountRoot })
    return response
  } catch (error) {
    log.warn('eject_volume IPC failed', { mountRoot, error })
    return { handled: false, message: null }
  }
}
