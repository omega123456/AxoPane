import {
  compressArchive,
  extractArchive,
  invokeNativeMenuAction,
  loadNativeMenu,
  openWith,
  showProperties,
} from '@/lib/ipc/commands'
import type {
  CompressArchiveRequest,
  ExtractArchiveRequest,
  InvokeNativeMenuRequest,
  LoadNativeMenuRequest,
  LoadNativeMenuResponse,
  MenuActionStatus,
  OpenWithRequest,
  ShowPropertiesRequest,
} from '@/lib/types/ipc'

export function requestNativeMenu(payload: LoadNativeMenuRequest): Promise<LoadNativeMenuResponse> {
  return loadNativeMenu(payload)
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

export function requestCompressArchive(payload: CompressArchiveRequest): Promise<MenuActionStatus> {
  return compressArchive(payload)
}

export function requestExtractArchive(payload: ExtractArchiveRequest): Promise<MenuActionStatus> {
  return extractArchive(payload)
}
