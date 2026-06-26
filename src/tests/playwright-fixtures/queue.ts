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
