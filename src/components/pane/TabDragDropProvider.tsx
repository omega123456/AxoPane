import { createContext, type ReactNode, useContext, useMemo, useRef, useState } from 'react'
import { move } from '@dnd-kit/helpers'
import { DragDropProvider, PointerSensor, useDroppable } from '@dnd-kit/react'
import { useSortable } from '@dnd-kit/react/sortable'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'

const TAB_TYPE = 'tab'

type TabDragState = {
  sourcePaneId: PaneId | null
  destinationPaneId: PaneId | null
  isInvalid: boolean
}

type TabDragContextValue = TabDragState

const TabDragContext = createContext<TabDragContextValue>({
  sourcePaneId: null,
  destinationPaneId: null,
  isInvalid: false,
})

type SourceDomPosition = {
  element: HTMLElement
  parent: HTMLElement
  nextSibling: Element | null
}

class MouseDistanceConstraint {
  private start: { x: number; y: number } | null = null
  private activateDrag: ((event: PointerEvent) => void) | null = null

  set controller(controller: { activate: (event: PointerEvent) => void }) {
    this.activateDrag = controller.activate.bind(controller)
  }

  onEvent(event: PointerEvent) {
    if (event.type === 'pointerdown') {
      this.start = { x: event.clientX, y: event.clientY }
      return
    }

    if (event.type === 'pointerup') {
      this.abort()
      return
    }

    if (event.type === 'pointermove' && this.start) {
      const distance = Math.hypot(event.clientX - this.start.x, event.clientY - this.start.y)
      if (distance >= 5) {
        this.activateDrag?.(event)
      }
    }
  }

  abort() {
    this.start = null
  }
}

const mouseSensor = PointerSensor.configure({
  preventActivation: (event) => event.pointerType !== 'mouse' || event.button !== 0,
  activationConstraints: (() => [new MouseDistanceConstraint()]) as never,
})

function isPaneId(value: unknown): value is PaneId {
  return value === 'left' || value === 'right'
}

function paneIdFromEntity(entity: { data: unknown; group?: unknown } | null | undefined) {
  if (!entity) {
    return null
  }

  const data = entity.data as { paneId?: unknown } | undefined
  if (isPaneId(data?.paneId)) {
    return data.paneId
  }

  return isPaneId(entity.group) ? entity.group : null
}

function paneIdAtPoint(x: number, y: number) {
  const strip = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-tab-strip]')
  return isPaneId(strip?.dataset.tabStrip) ? strip.dataset.tabStrip : null
}

function validDestination(sourcePaneId: PaneId, destinationPaneId: PaneId) {
  return (
    sourcePaneId === destinationPaneId ||
    useTabsStore.getState().panes[sourcePaneId].tabs.length > 1
  )
}

export function useTabDragState() {
  return useContext(TabDragContext)
}

export function useTabSortable(paneId: PaneId, tabId: string, index: number) {
  return useSortable({
    id: tabId,
    group: paneId,
    index,
    type: TAB_TYPE,
    accept: (source) => {
      const sourcePaneId = paneIdFromEntity(source)
      return sourcePaneId !== null && validDestination(sourcePaneId, paneId)
    },
    collisionPriority: 1,
    data: { paneId },
  })
}

export function useTabStripDropTarget(paneId: PaneId) {
  return useDroppable({
    id: `tab-strip-${paneId}`,
    type: TAB_TYPE,
    accept: (source) => {
      const sourcePaneId = paneIdFromEntity(source)
      return sourcePaneId !== null && validDestination(sourcePaneId, paneId)
    },
    collisionPriority: 0,
    data: { paneId, isStrip: true },
  })
}

type TabDragDropProviderProps = {
  children: ReactNode
}

export function TabDragDropProvider({ children }: TabDragDropProviderProps) {
  const moveTab = usePanesStore((state) => state.moveTab)
  const tabs = useTabsStore((state) => state.panes)
  const [dragState, setDragState] = useState<TabDragState>({
    sourcePaneId: null,
    destinationPaneId: null,
    isInvalid: false,
  })
  const sourceTabId = useRef<string | null>(null)
  const sourceDomPosition = useRef<SourceDomPosition | null>(null)

  const tabIds = useMemo(
    () => ({
      left: tabs.left.tabs.map((tab) => tab.id),
      right: tabs.right.tabs.map((tab) => tab.id),
    }),
    [tabs.left.tabs, tabs.right.tabs],
  )

  const reset = () => {
    sourceDomPosition.current = null
    setDragState({ sourcePaneId: null, destinationPaneId: null, isInvalid: false })
  }

  const restoreSourceDomPosition = () => {
    const position = sourceDomPosition.current
    if (!position) return

    const placeholder = document.querySelector<HTMLElement>('[data-dnd-placeholder]')
    const anchor =
      position.nextSibling?.parentElement === position.parent ? position.nextSibling : null
    position.parent.insertBefore(position.element, anchor)
    if (placeholder) {
      position.element.insertAdjacentElement('afterend', placeholder)
    }
  }

  const restoreSourceLabelFocus = () => {
    const tabId = sourceTabId.current
    if (tabId) {
      document.querySelector<HTMLElement>(`[data-tab-label-id="${CSS.escape(tabId)}"]`)?.focus()
    }
  }

  return (
    <DragDropProvider
      sensors={[mouseSensor]}
      onDragStart={(event) => {
        const sourcePaneId = paneIdFromEntity(event.operation.source)
        sourceTabId.current =
          typeof event.operation.source?.id === 'string' ? event.operation.source.id : null
        const sourceElement = sourceTabId.current
          ? document.querySelector<HTMLElement>(
              `[data-tab-id="${CSS.escape(sourceTabId.current)}"]`,
            )
          : null
        sourceDomPosition.current =
          sourceElement?.parentElement instanceof HTMLElement
            ? {
                element: sourceElement,
                parent: sourceElement.parentElement,
                nextSibling: sourceElement.nextElementSibling,
              }
            : null
        setDragState({ sourcePaneId, destinationPaneId: sourcePaneId, isInvalid: false })
      }}
      onDragMove={(event) => {
        const sourcePaneId = paneIdFromEntity(event.operation.source)
        const destinationPaneId = event.to && paneIdAtPoint(event.to.x, event.to.y)
        if (
          !sourcePaneId ||
          !destinationPaneId ||
          validDestination(sourcePaneId, destinationPaneId)
        ) {
          return
        }

        setDragState({ sourcePaneId, destinationPaneId: null, isInvalid: true })
      }}
      onDragOver={(event) => {
        const sourcePaneId = paneIdFromEntity(event.operation.source)
        const target = event.operation.target
        const destinationPaneId = paneIdFromEntity(target)
        const isInvalid =
          sourcePaneId !== null &&
          destinationPaneId !== null &&
          sourcePaneId !== destinationPaneId &&
          !validDestination(sourcePaneId, destinationPaneId)
        setDragState({
          sourcePaneId,
          destinationPaneId: isInvalid ? null : destinationPaneId,
          isInvalid,
        })
        if (isInvalid) {
          event.preventDefault()
        }
      }}
      onDragEnd={(event) => {
        const source = event.operation.source
        const target = event.operation.target
        const sourcePaneId = paneIdFromEntity(source)
        const targetPaneId = paneIdFromEntity(target)
        const sourceTabId = source?.id

        if (
          !event.canceled &&
          sourcePaneId &&
          targetPaneId &&
          typeof sourceTabId === 'string' &&
          validDestination(sourcePaneId, targetPaneId)
        ) {
          const projected = move(tabIds, event)
          const projectedPaneId = (Object.keys(projected) as PaneId[]).find((paneId) =>
            projected[paneId].includes(sourceTabId),
          )
          const targetData = target?.data as { isStrip?: boolean } | undefined
          const destinationPaneId = targetData?.isStrip
            ? targetPaneId
            : (projectedPaneId ?? targetPaneId)
          const destinationIndex = targetData?.isStrip
            ? tabs[destinationPaneId].tabs.length
            : projected[destinationPaneId].indexOf(sourceTabId)

          if (destinationIndex >= 0) {
            if (sourcePaneId !== destinationPaneId) {
              restoreSourceDomPosition()
            }
            void moveTab(sourcePaneId, sourceTabId, destinationPaneId, destinationIndex).then(
              (result) => {
                if (result.kind === 'transfer') {
                  requestAnimationFrame(() => {
                    document
                      .querySelector<HTMLElement>(
                        `[data-tab-label-id="${CSS.escape(result.destinationTabId)}"]`,
                      )
                      ?.focus()
                  })
                } else if (result.kind === 'none') {
                  restoreSourceLabelFocus()
                }
              },
            )
          } else {
            restoreSourceLabelFocus()
          }
        } else {
          restoreSourceLabelFocus()
        }

        reset()
      }}
    >
      <TabDragContext.Provider value={dragState}>{children}</TabDragContext.Provider>
    </DragDropProvider>
  )
}
