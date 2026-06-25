import type { OpProgress, OpSnapshot } from '@/lib/types/ipc'

/**
 * Phase 6 queue screenshot fixtures. Specs select one via
 * `window.__PLAYWRIGHT_IPC_OVERRIDES__` so the screenshot router stays free of
 * inline domain data.
 */

function activeCopy(): OpProgress {
  return {
    operationId: 'op-1',
    kind: 'copy',
    status: 'active',
    sourceDir: 'D:\\Media\\Archives',
    destinationDir: 'E:\\Cold-Storage\\2025',
    totalItems: 1248,
    completedItems: 812,
    totalBytes: 1000,
    copiedBytes: 630,
    progressPercent: 63,
    bytesPerSecond: 260_046_848,
    etaSeconds: 180,
    currentFileName: 'master-reel-final.mkv',
    currentFileCopiedBytes: 12_240_000_000,
    currentFileTotalBytes: 19_760_000_000,
    errorMessage: null,
  }
}

export const collapsedQueueSnapshot: OpSnapshot[] = [{ progress: activeCopy(), conflict: null }]

export const emptyQueueSnapshot: OpSnapshot[] = []

export const expandedQueueSnapshot: OpSnapshot[] = [
  { progress: activeCopy(), conflict: null },
  {
    progress: {
      operationId: 'op-2',
      kind: 'move',
      status: 'pending',
      sourceDir: 'C:\\Downloads',
      destinationDir: 'D:\\Sorted',
      totalItems: 32,
      completedItems: 0,
      totalBytes: 500,
      copiedBytes: 0,
      progressPercent: 0,
      bytesPerSecond: 0,
      etaSeconds: null,
      currentFileName: null,
      currentFileCopiedBytes: 0,
      currentFileTotalBytes: 0,
      errorMessage: null,
    },
    conflict: null,
  },
]

export const conflictQueueSnapshot: OpSnapshot[] = [
  {
    progress: {
      operationId: 'op-1',
      kind: 'copy',
      status: 'conflict',
      sourceDir: 'D:\\Media\\Archives',
      destinationDir: 'E:\\Cold-Storage\\2025',
      totalItems: 1248,
      completedItems: 120,
      totalBytes: 1000,
      copiedBytes: 96,
      progressPercent: 9.6,
      bytesPerSecond: 120_000_000,
      etaSeconds: null,
      currentFileName: 'master-reel-final.mkv',
      currentFileCopiedBytes: 0,
      currentFileTotalBytes: 19_760_000_000,
      errorMessage: null,
    },
    conflict: {
      operationId: 'op-1',
      sourcePath: 'D:\\Media\\Archives\\master-reel-final.mkv',
      destinationPath: 'E:\\Cold-Storage\\2025\\master-reel-final.mkv',
      name: 'master-reel-final.mkv',
    },
  },
]
