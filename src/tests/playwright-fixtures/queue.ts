import type { OpProgress, OpSnapshot } from '@/lib/types/ipc'

/**
 * Phase 6 queue screenshot fixtures. Specs select one via
 * `window.__PLAYWRIGHT_IPC_OVERRIDES__` so the screenshot router stays free of
 * inline domain data.
 */

function activeCopy(overrides: Partial<OpProgress> = {}): OpProgress {
  return {
    operationId: 'op-1',
    kind: 'copy',
    status: 'active',
    sourceDir: 'D:\\Media\\Archives',
    itemNames: ['Q3-conference-highlights', 'master-reel-final.mkv'],
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
    ...overrides,
  }
}

export const expandedQueueProgressEvents: OpProgress[] = [
  activeCopy({
    completedItems: 78,
    copiedBytes: 60,
    progressPercent: 6,
    bytesPerSecond: 110_100_480,
    etaSeconds: 880,
    currentFileCopiedBytes: 1_180_000_000,
  }),
  activeCopy({
    completedItems: 156,
    copiedBytes: 122,
    progressPercent: 12.2,
    bytesPerSecond: 188_743_680,
    etaSeconds: 812,
    currentFileCopiedBytes: 2_410_000_000,
  }),
  activeCopy({
    completedItems: 226,
    copiedBytes: 176,
    progressPercent: 17.6,
    bytesPerSecond: 255_852_544,
    etaSeconds: 744,
    currentFileCopiedBytes: 3_470_000_000,
  }),
  activeCopy({
    completedItems: 298,
    copiedBytes: 232,
    progressPercent: 23.2,
    bytesPerSecond: 211_812_352,
    etaSeconds: 662,
    currentFileCopiedBytes: 4_590_000_000,
  }),
  activeCopy({
    completedItems: 372,
    copiedBytes: 290,
    progressPercent: 29,
    bytesPerSecond: 304_087_040,
    etaSeconds: 584,
    currentFileCopiedBytes: 5_720_000_000,
  }),
  activeCopy({
    completedItems: 444,
    copiedBytes: 346,
    progressPercent: 34.6,
    bytesPerSecond: 241_172_480,
    etaSeconds: 516,
    currentFileCopiedBytes: 6_830_000_000,
  }),
  activeCopy({
    completedItems: 514,
    copiedBytes: 401,
    progressPercent: 40.1,
    bytesPerSecond: 335_544_320,
    etaSeconds: 456,
    currentFileCopiedBytes: 7_920_000_000,
  }),
  activeCopy({
    completedItems: 582,
    copiedBytes: 454,
    progressPercent: 45.4,
    bytesPerSecond: 201_326_592,
    etaSeconds: 398,
    currentFileCopiedBytes: 8_960_000_000,
  }),
  activeCopy({
    completedItems: 646,
    copiedBytes: 504,
    progressPercent: 50.4,
    bytesPerSecond: 283_115_520,
    etaSeconds: 342,
    currentFileCopiedBytes: 9_940_000_000,
  }),
  activeCopy({
    completedItems: 690,
    copiedBytes: 538,
    progressPercent: 53.8,
    bytesPerSecond: 197_132_288,
    etaSeconds: 304,
    currentFileCopiedBytes: 10_610_000_000,
  }),
  activeCopy({
    completedItems: 714,
    copiedBytes: 550,
    progressPercent: 55,
    bytesPerSecond: 163_577_856,
    etaSeconds: 264,
    currentFileCopiedBytes: 10_840_000_000,
  }),
  activeCopy({
    completedItems: 736,
    copiedBytes: 568,
    progressPercent: 56.8,
    bytesPerSecond: 239_075_328,
    etaSeconds: 238,
    currentFileCopiedBytes: 11_180_000_000,
  }),
  activeCopy({
    completedItems: 758,
    copiedBytes: 586,
    progressPercent: 58.6,
    bytesPerSecond: 293_601_280,
    etaSeconds: 218,
    currentFileCopiedBytes: 11_540_000_000,
  }),
  activeCopy({
    completedItems: 776,
    copiedBytes: 600,
    progressPercent: 60,
    bytesPerSecond: 230_686_720,
    etaSeconds: 205,
    currentFileCopiedBytes: 11_900_000_000,
  }),
  activeCopy({
    completedItems: 792,
    copiedBytes: 612,
    progressPercent: 61.2,
    bytesPerSecond: 180_355_072,
    etaSeconds: 194,
    currentFileCopiedBytes: 12_130_000_000,
  }),
  activeCopy({
    completedItems: 802,
    copiedBytes: 620,
    progressPercent: 62,
    bytesPerSecond: 209_715_200,
    etaSeconds: 188,
    currentFileCopiedBytes: 12_220_000_000,
  }),
  activeCopy({
    completedItems: 808,
    copiedBytes: 626,
    progressPercent: 62.6,
    bytesPerSecond: 241_172_480,
    etaSeconds: 184,
    currentFileCopiedBytes: 12_235_000_000,
  }),
  activeCopy(),
]

export const expandedQueueFinalProgressEvent =
  expandedQueueProgressEvents[expandedQueueProgressEvents.length - 1]

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
      itemNames: ['Season 01', 'poster.jpg', 'notes.txt'],
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

export const deletingQueueProgressEvents: OpProgress[] = [
  activeCopy({
    operationId: 'op-delete',
    kind: 'delete',
    sourceDir: 'C:\\Users\\Omega\\Downloads',
    itemNames: ['old-export.zip', 'scratch'],
    destinationDir: '',
    totalItems: 84,
    completedItems: 8,
    totalBytes: 1000,
    copiedBytes: 95,
    progressPercent: 9.5,
    bytesPerSecond: 42_991_616,
    etaSeconds: 86,
    currentFileName: 'scratch\\cache.bin',
    currentFileCopiedBytes: 95_000_000,
    currentFileTotalBytes: 1_000_000_000,
  }),
  activeCopy({
    operationId: 'op-delete',
    kind: 'delete',
    sourceDir: 'C:\\Users\\Omega\\Downloads',
    itemNames: ['old-export.zip', 'scratch'],
    destinationDir: '',
    totalItems: 84,
    completedItems: 17,
    totalBytes: 1000,
    copiedBytes: 205,
    progressPercent: 20.5,
    bytesPerSecond: 62_914_560,
    etaSeconds: 69,
    currentFileName: 'scratch\\cache.bin',
    currentFileCopiedBytes: 205_000_000,
    currentFileTotalBytes: 1_000_000_000,
  }),
  activeCopy({
    operationId: 'op-delete',
    kind: 'delete',
    sourceDir: 'C:\\Users\\Omega\\Downloads',
    itemNames: ['old-export.zip', 'scratch'],
    destinationDir: '',
    totalItems: 84,
    completedItems: 28,
    totalBytes: 1000,
    copiedBytes: 330,
    progressPercent: 33,
    bytesPerSecond: 86_507_520,
    etaSeconds: 56,
    currentFileName: 'scratch\\cache.bin',
    currentFileCopiedBytes: 330_000_000,
    currentFileTotalBytes: 1_000_000_000,
  }),
  activeCopy({
    operationId: 'op-delete',
    kind: 'delete',
    sourceDir: 'C:\\Users\\Omega\\Downloads',
    itemNames: ['old-export.zip', 'scratch'],
    destinationDir: '',
    totalItems: 84,
    completedItems: 37,
    totalBytes: 1000,
    copiedBytes: 440,
    progressPercent: 44,
    bytesPerSecond: 72_351_744,
    etaSeconds: 48,
    currentFileName: 'scratch\\cache.bin',
    currentFileCopiedBytes: 440_000_000,
    currentFileTotalBytes: 1_000_000_000,
  }),
]

export const deletingQueueFinalProgressEvent =
  deletingQueueProgressEvents[deletingQueueProgressEvents.length - 1]

export const deletingQueueSnapshot: OpSnapshot[] = [
  {
    progress:
      deletingQueueProgressEvents[0] ??
      activeCopy({
        operationId: 'op-delete',
        kind: 'delete',
        sourceDir: 'C:\\Users\\Omega\\Downloads',
        itemNames: ['old-export.zip', 'scratch'],
        destinationDir: '',
        totalItems: 84,
        completedItems: 37,
        totalBytes: 1000,
        copiedBytes: 440,
        progressPercent: 44,
        bytesPerSecond: 72_351_744,
        etaSeconds: 48,
        currentFileName: 'scratch\\cache.bin',
        currentFileCopiedBytes: 440_000_000,
        currentFileTotalBytes: 1_000_000_000,
      }),
    conflict: null,
  },
]

export const expandedQueueSeedSnapshot: OpSnapshot[] = [
  {
    progress: expandedQueueProgressEvents[0],
    conflict: null,
  },
  expandedQueueSnapshot[1],
]

export const conflictQueueSnapshot: OpSnapshot[] = [
  {
    progress: {
      operationId: 'op-1',
      kind: 'copy',
      status: 'conflict',
      sourceDir: 'D:\\Media\\Archives',
      itemNames: ['master-reel-final.mkv', 'b-roll-package.mov'],
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

// Regression fixture: the job card appends the top-level item name to the
// source path (e.g. `sourceDir\itemName`). Both the path and the item name
// itself need to stay readable (truncated, not overflowing) even when they
// are far longer than any realistic single path segment.
export const longPathQueueSnapshot: OpSnapshot[] = [
  {
    progress: {
      operationId: 'op-1',
      kind: 'copy',
      status: 'active',
      sourceDir:
        'D:\\Users\\Omega\\Documents\\Projects\\Client-Deliverables\\2025\\Q3\\Video-Production\\Raw-Footage\\Uncompressed',
      itemNames: [
        'Final-Master-Export-Uncompressed-ProRes-4444-Director-Approved-v12-DO-NOT-DELETE',
      ],
      destinationDir:
        'F:\\Backups\\Archive\\2025\\Video-Production\\Client-Deliverables\\Cold-Storage\\Long-Term-Retention',
      totalItems: 1,
      completedItems: 0,
      totalBytes: 1000,
      copiedBytes: 630,
      progressPercent: 63,
      bytesPerSecond: 260_046_848,
      etaSeconds: 180,
      currentFileName:
        'Final-Master-Export-Uncompressed-ProRes-4444-Director-Approved-v12-DO-NOT-DELETE.mov',
      currentFileCopiedBytes: 12_240_000_000,
      currentFileTotalBytes: 19_760_000_000,
      errorMessage: null,
    },
    conflict: null,
  },
]

function pendingMove(operationId: string): OpSnapshot {
  return {
    progress: {
      operationId,
      kind: 'move',
      status: 'pending',
      sourceDir: 'C:\\Downloads',
      itemNames: ['Season 01', 'poster.jpg', 'notes.txt'],
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
  }
}

// Regression fixture: the queued-jobs list used to only have room for a
// single pending job below the active one before requiring a scroll. With
// several pending jobs queued up, at least 3 should be visible without
// scrolling on a normal-height window.
export const manyPendingQueueSnapshot: OpSnapshot[] = [
  { progress: activeCopy(), conflict: null },
  pendingMove('op-2'),
  pendingMove('op-3'),
  pendingMove('op-4'),
  pendingMove('op-5'),
  pendingMove('op-6'),
]
