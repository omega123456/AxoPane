export type PendingScrollRestore = {
  target: number
  justArmed: boolean
}

export type ScrollMeasurement = {
  scrollTop: number
  scrollHeight: number
  clientHeight: number
}

const SUB_PIXEL_TOLERANCE_PX = 1

export function armedScrollRestore(target: number): PendingScrollRestore {
  return { target, justArmed: true }
}

function reachedTarget(scrollTop: number, target: number) {
  const roundedNearTarget = Math.abs(scrollTop - target) < SUB_PIXEL_TOLERANCE_PX
  return scrollTop >= target || roundedNearTarget
}

function clampedWithNoRoomToGrow(measurement: ScrollMeasurement, target: number) {
  const contentFitsViewport = measurement.scrollHeight <= measurement.clientHeight
  return measurement.scrollTop < target || contentFitsViewport
}

function isRestoreSettled(restore: PendingScrollRestore, measurement: ScrollMeasurement) {
  if (reachedTarget(measurement.scrollTop, restore.target)) {
    return true
  }
  const retriedAtLeastOnce = !restore.justArmed
  return retriedAtLeastOnce && clampedWithNoRoomToGrow(measurement, restore.target)
}

export function scrollRestoreAfterAttempt(
  restore: PendingScrollRestore,
  measurement: ScrollMeasurement,
): PendingScrollRestore | null {
  if (isRestoreSettled(restore, measurement)) {
    return null
  }
  return { target: restore.target, justArmed: false }
}
