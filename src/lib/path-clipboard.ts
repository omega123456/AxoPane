import { log } from '@/lib/app-log-commands'

function formatClipboardPayload(paths: string[]) {
  return paths.join('\n')
}

export async function copyPathsToClipboard(paths: string[]) {
  if (paths.length === 0) {
    return
  }

  if (!navigator.clipboard?.writeText) {
    log.warn('path clipboard unavailable', { paths })
    return
  }

  try {
    await navigator.clipboard.writeText(formatClipboardPayload(paths))
    log.info(paths.length === 1 ? 'Copied path' : 'Copied paths', {
      path: paths.length === 1 ? paths[0] : undefined,
      paths: paths.length > 1 ? paths : undefined,
    })
  } catch (error) {
    log.warn('copy path failed', {
      path: paths.length === 1 ? paths[0] : undefined,
      paths: paths.length > 1 ? paths : undefined,
      error,
    })
  }
}
