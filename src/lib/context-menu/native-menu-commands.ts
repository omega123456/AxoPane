import {
  invokeNativeMenuAction,
  loadNativeMenu,
  openWith,
  showProperties,
  warmNativeMenus,
} from '@/lib/ipc/commands'
import { startOp } from '@/lib/queue-commands'
import type {
  CompressArchiveRequest,
  ExtractArchiveRequest,
  InvokeNativeMenuRequest,
  LoadNativeMenuRequest,
  LoadNativeMenuResponse,
  MenuActionStatus,
  StartOpRequest,
  OpenWithRequest,
  ShowPropertiesRequest,
  WarmNativeMenusRequest,
} from '@/lib/types/ipc'

export function requestNativeMenu(payload: LoadNativeMenuRequest): Promise<LoadNativeMenuResponse> {
  return loadNativeMenu(payload)
}

export function warmNativeMenu(payload: WarmNativeMenusRequest): Promise<void> {
  return warmNativeMenus(payload)
}

export function invokeNativeMenu(payload: InvokeNativeMenuRequest): Promise<MenuActionStatus> {
  return invokeNativeMenuAction(payload)
}

export function showNativeProperties(payload: ShowPropertiesRequest): Promise<MenuActionStatus> {
  return showProperties(payload)
}

export function showNativeOpenWith(payload: OpenWithRequest): Promise<MenuActionStatus> {
  return openWith(payload)
}

function archiveItems(paths: string[]) {
  return paths.map((sourcePath) => ({
    sourcePath,
    name: sourcePath.replace(/\\/g, '/').split('/').filter(Boolean).at(-1) || 'Archive',
    // The queue discovers archive sizes progressively; callers must not scan.
    sizeBytes: 0,
  }))
}

async function requestArchiveOperation(
  kind: StartOpRequest['kind'],
  payload: CompressArchiveRequest | ExtractArchiveRequest,
): Promise<string> {
  return startOp({ kind, destinationDir: payload.destinationDir, items: archiveItems(payload.paths) })
}

export function requestCompressArchive(payload: CompressArchiveRequest): Promise<string> {
  return requestArchiveOperation('compress', payload)
}

export function requestExtractArchive(payload: ExtractArchiveRequest): Promise<string> {
  return requestArchiveOperation('extract', payload)
}
