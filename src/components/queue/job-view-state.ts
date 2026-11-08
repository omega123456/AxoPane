export function retainedCurrentFile(lastKnownFile: string | null, currentFile: string | null) {
  const fileChanged = currentFile !== null && currentFile !== lastKnownFile
  return fileChanged ? currentFile : lastKnownFile
}

export function isDiscoveringBytes(latestTotalBytes: number, previousTotalBytes: number) {
  return latestTotalBytes > previousTotalBytes
}

export function announcementToPublish(pending: string | null, published: string) {
  const hasNewAnnouncement = pending !== null && pending !== published
  return hasNewAnnouncement ? pending : null
}
