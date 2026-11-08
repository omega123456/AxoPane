import type { ComponentProps } from 'react'
import { move } from '@dnd-kit/helpers'
import type { DragDropProvider } from '@dnd-kit/react'
import { useTabsStore } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'

function isPaneId(value: unknown): value is PaneId {
  return value === 'left' || value === 'right'
}

export function paneIdFromEntity(entity: { data: unknown; group?: unknown } | null | undefined) {
  if (!entity) {
    return null
  }

  const data = entity.data as { paneId?: unknown } | undefined
  if (isPaneId(data?.paneId)) {
    return data.paneId
  }

  return isPaneId(entity.group) ? entity.group : null
}

export function paneIdAtPoint(clientX: number, clientY: number) {
  const strip = document
    .elementFromPoint(clientX, clientY)
    ?.closest<HTMLElement>('[data-tab-strip]')
  return isPaneId(strip?.dataset.tabStrip) ? strip.dataset.tabStrip : null
}

export function validDestination(sourcePaneId: PaneId, destinationPaneId: PaneId) {
  return (
    sourcePaneId === destinationPaneId ||
    useTabsStore.getState().panes[sourcePaneId].tabs.length > 1
  )
}

export type TabDragEndEvent = Parameters<
  NonNullable<ComponentProps<typeof DragDropProvider>['onDragEnd']>
>[0]

export type TabIdsByPane = Record<PaneId, string[]>

export type TabDrop = {
  sourcePaneId: PaneId
  sourceTabId: string
  destinationPaneId: PaneId
  destinationIndex: number
}

export function focusTabLabel(tabId: string) {
  document.querySelector<HTMLElement>(`[data-tab-label-id="${CSS.escape(tabId)}"]`)?.focus()
}

function isStripTarget(target: TabDragEndEvent['operation']['target']) {
  return Boolean((target?.data as { isStrip?: boolean } | undefined)?.isStrip)
}

function tabDropDestination(
  event: TabDragEndEvent,
  sourceTabId: string,
  targetPaneId: PaneId,
  tabIds: TabIdsByPane,
): Pick<TabDrop, 'destinationPaneId' | 'destinationIndex'> {
  if (isStripTarget(event.operation.target)) {
    return { destinationPaneId: targetPaneId, destinationIndex: tabIds[targetPaneId].length }
  }
  const projected = move(tabIds, event)
  const destinationPaneId =
    (Object.keys(projected) as PaneId[]).find((paneId) =>
      projected[paneId].includes(sourceTabId),
    ) ?? targetPaneId
  return { destinationPaneId, destinationIndex: projected[destinationPaneId].indexOf(sourceTabId) }
}

export function resolveTabDrop(event: TabDragEndEvent, tabIds: TabIdsByPane): TabDrop | null {
  const { source, target } = event.operation
  const sourcePaneId = paneIdFromEntity(source)
  const targetPaneId = paneIdFromEntity(target)
  const sourceTabId = source?.id
  if (event.canceled) {
    return null
  }
  if (!sourcePaneId) {
    return null
  }
  if (!targetPaneId) {
    return null
  }
  if (typeof sourceTabId !== 'string') {
    return null
  }
  if (!validDestination(sourcePaneId, targetPaneId)) {
    return null
  }
  const destination = tabDropDestination(event, sourceTabId, targetPaneId, tabIds)
  if (destination.destinationIndex < 0) {
    return null
  }
  return { sourcePaneId, sourceTabId, ...destination }
}
