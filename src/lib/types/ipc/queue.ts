export type OpKind = 'copy' | 'move' | 'delete' | 'compress' | 'extract'

export type OpStatus =
  | 'pending'
  | 'active'
  | 'paused'
  | 'conflict'
  | 'completed'
  | 'failed'
  | 'cancelled'

export type ConflictResolution = 'replace' | 'skip' | 'rename'

export type OpItem = {
  sourcePath: string
  name: string
  sizeBytes: number
}

export type StartOpRequest = {
  kind: OpKind
  destinationDir: string
  items: OpItem[]
}

export type ConflictInfo = {
  operationId: string
  sourcePath: string
  destinationPath: string
  name: string
}

export type SetWindowThemeRequest = {
  theme: 'light' | 'dark' | null
}

export type WindowBackdrop = 'mica' | 'solid'

export type OpProgress = {
  operationId: string
  kind: OpKind
  status: OpStatus
  sourceDir: string
  itemNames: string[]
  destinationDir: string
  totalItems: number
  completedItems: number
  totalBytes: number
  copiedBytes: number
  progressPercent: number
  bytesPerSecond: number
  etaSeconds: number | null
  currentFileName: string | null
  currentFileCopiedBytes: number
  currentFileTotalBytes: number
  errorMessage: string | null
}

export type ThroughputSample = {
  percent: number
  rate: number
}

export type OpSnapshot = {
  progress: OpProgress
  conflict: ConflictInfo | null
}

export type QueueProgressEvent = OpProgress

export type QueueConflictEvent = ConflictInfo

export type QueueRemovedEvent = string

export type OpIdRequest = {
  id: string
}

export type ReorderOpsRequest = {
  ids: string[]
}

export type ResolveConflictRequest = {
  id: string
  resolution: ConflictResolution
  applyToAll: boolean
  renameTo: string | null
}

export type WatchErrorEvent = {
  path: string
  message: string
}

export type FrontendLogLevel = 'debug' | 'info' | 'warn' | 'error'

export type LogFrontendRequest = {
  level: FrontendLogLevel
  message: string
  category?: string
  details?: string
}
