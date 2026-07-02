import {
  compressArchive,
  extractArchive,
  invokeNativeMenuAction,
  loadNativeMenu,
  openWith,
  showProperties,
  warmNativeMenus,
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

export function requestCompressArchive(payload: CompressArchiveRequest): Promise<MenuActionStatus> {
  return compressArchive(payload)
}

export function requestExtractArchive(payload: ExtractArchiveRequest): Promise<MenuActionStatus> {
  return extractArchive(payload)
}
