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
    const response = await requestCompressArchive({ paths, destinationDir })
    if (!response.handled) {
      log.info('compress command unavailable', {
        ...summarize(paths),
        destinationDir,
        message: response.message ?? null,
      })
      return response
    }

    log.info('compress command completed', {
      ...summarize(paths),
      destinationDir,
      archivePath: response.message ?? null,
    })
    return response
  } catch (error) {
    log.warn('compress_archive IPC failed', {
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
    const response = await requestExtractArchive({ paths, destinationDir })
    if (!response.handled) {
      log.info('extract command unavailable', {
        ...summarize(paths),
        destinationDir,
        message: response.message ?? null,
      })
      return response
    }

    log.info('extract command completed', {
      ...summarize(paths),
      destinationDir,
      extractedPath: response.message ?? null,
    })
    return response
  } catch (error) {
    log.warn('extract_archive IPC failed', {
      ...summarize(paths),
      destinationDir,
      error,
    })
    return null
  }
}
