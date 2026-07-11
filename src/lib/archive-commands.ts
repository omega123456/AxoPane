import { log } from '@/lib/app-log-commands'
import {
  requestCompressArchive,
  requestExtractArchive,
} from '@/lib/context-menu/native-menu-commands'

type ArchiveActionRequest = {
  paths: string[]
  destinationDir: string
}

function summarize(paths: string[]) {
  return paths.length === 1 ? { path: paths[0] } : { count: paths.length, paths }
}

export async function runCompressCommand({ paths, destinationDir }: ArchiveActionRequest) {
  if (paths.length === 0) {
    log.info('compress requested without any targets')
    return null
  }

  try {
    const operationId = await requestCompressArchive({ paths, destinationDir })

    log.info('compress command queued', {
      ...summarize(paths),
      destinationDir,
      operationId,
    })
    return operationId
  } catch (error) {
    log.warn('compress archive queue submission failed', {
      ...summarize(paths),
      destinationDir,
      error,
    })
    return null
  }
}

export async function runExtractCommand({ paths, destinationDir }: ArchiveActionRequest) {
  if (paths.length === 0) {
    log.info('extract requested without any targets')
    return null
  }

  try {
    const operationId = await requestExtractArchive({ paths, destinationDir })

    log.info('extract command queued', {
      ...summarize(paths),
      destinationDir,
      operationId,
    })
    return operationId
  } catch (error) {
    log.warn('extract archive queue submission failed', {
      ...summarize(paths),
      destinationDir,
      error,
    })
    return null
  }
}
