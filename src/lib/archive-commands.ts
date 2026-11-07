import { log } from '@/lib/app-log-commands'
import {
  requestCompressArchive,
  requestExtractArchive,
} from '@/lib/context-menu/native-menu-commands'

type ArchiveActionRequest = {
  paths: string[]
  destinationDir: string
}

type ArchiveSubmitter = (request: ArchiveActionRequest) => Promise<string>

function summarize(paths: string[]) {
  return paths.length === 1 ? { path: paths[0] } : { count: paths.length, paths }
}

async function queueArchiveAction(
  action: 'compress' | 'extract',
  submit: ArchiveSubmitter,
  { paths, destinationDir }: ArchiveActionRequest,
) {
  if (paths.length === 0) {
    log.info(`${action} requested without any targets`)
    return null
  }

  try {
    const operationId = await submit({ paths, destinationDir })
    log.info(`${action} command queued`, { ...summarize(paths), destinationDir, operationId })
    return operationId
  } catch (error) {
    log.warn(`${action} archive queue submission failed`, {
      ...summarize(paths),
      destinationDir,
      error,
    })
    return null
  }
}

export function runCompressCommand(request: ArchiveActionRequest) {
  return queueArchiveAction('compress', requestCompressArchive, request)
}

export function runExtractCommand(request: ArchiveActionRequest) {
  return queueArchiveAction('extract', requestExtractArchive, request)
}
