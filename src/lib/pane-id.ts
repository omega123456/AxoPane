import type { PaneId } from '@/types/pane'

const oppositePane: Record<PaneId, PaneId> = {
  left: 'right',
  right: 'left',
}

export function otherPane(paneId: PaneId): PaneId {
  return oppositePane[paneId]
}
