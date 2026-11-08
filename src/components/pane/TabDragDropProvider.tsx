import { createContext, type ReactNode, useContext, useMemo, useRef, useState } from 'react'
import {
  DragDropProvider,
  PointerSensor,
  useDroppable,
  type DragMoveEvent,
  type DragOverEvent,
  type DragStartEvent,
} from '@dnd-kit/react'
import { useSortable } from '@dnd-kit/react/sortable'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore, type TabMoveResult } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'
import {
  focusTabLabel,
  paneIdAtPoint,
  paneIdFromEntity,
  resolveTabDrop,
  validDestination,
  type TabDrop,
} from './tab-drop'

const TAB_DRAG_ACTIVATION_DISTANCE_PX = 5

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

    if (event.type !== 'pointermove') {
      return
    }
    if (!this.start) {
      return
    }
    const distance = Math.hypot(event.clientX - this.start.x, event.clientY - this.start.y)
    if (distance >= TAB_DRAG_ACTIVATION_DISTANCE_PX) {
      this.activateDrag?.(event)
    }
  }

  abort() {
    this.start = null
  }
}

type PointerSensorOptions = Parameters<typeof PointerSensor.configure>[0]
type PointerActivationConstraintsFactory = NonNullable<
  PointerSensorOptions['activationConstraints']
>

// dnd-kit's ActivationConstraint base class carries a #private brand and is not exported, so a duck-typed constraint needs a cast.
const mouseDistanceActivation = (() => [
  new MouseDistanceConstraint(),
]) as unknown as PointerActivationConstraintsFactory

const mouseSensor = PointerSensor.configure({
  preventActivation: (event) => event.pointerType !== 'mouse' || event.button !== 0,
  activationConstraints: mouseDistanceActivation,
})

function compensateDragZoom(element: HTMLElement) {
  const zoom = Number.parseFloat(document.documentElement.style.zoom) || 1
  if (zoom === 1) return

  // DnD renders viewport geometry back inside the zoomed root. Counter-scale
  // only its fixed-position feedback so that geometry stays in viewport pixels.
  const previousZoom = element.style.zoom
  element.style.zoom = String(1 / zoom)
  const observer = new MutationObserver(() => {
    if (!element.hasAttribute('data-dnd-dragging')) restore()
  })
  const restore = () => {
    observer.disconnect()
    element.style.zoom = previousZoom
  }
  observer.observe(element, { attributes: true, attributeFilter: ['data-dnd-dragging'] })
  return restore
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

const IDLE_DRAG_STATE: TabDragState = {
  sourcePaneId: null,
  destinationPaneId: null,
  isInvalid: false,
}

function sourceTabIdOf(event: DragStartEvent) {
  const id = event.operation.source?.id
  return typeof id === 'string' ? id : null
}

function domPositionOf(element: HTMLElement | null): SourceDomPosition | null {
  if (!(element?.parentElement instanceof HTMLElement)) {
    return null
  }
  return { element, parent: element.parentElement, nextSibling: element.nextElementSibling }
}

function isStillDragging(position: SourceDomPosition | null) {
  return position?.element.hasAttribute('data-dnd-dragging') === true
}

function restoreDomPosition(position: SourceDomPosition | null) {
  if (!position) return

  const placeholder = document.querySelector<HTMLElement>('[data-dnd-placeholder]')
  const siblingStillInParent = position.nextSibling?.parentElement === position.parent
  const anchor = siblingStillInParent ? position.nextSibling : null
  position.parent.insertBefore(position.element, anchor)
  if (placeholder) {
    position.element.insertAdjacentElement('afterend', placeholder)
  }
}

function invalidMoveState(event: DragMoveEvent): TabDragState | null {
  const sourcePaneId = paneIdFromEntity(event.operation.source)
  const destinationPaneId = event.to && paneIdAtPoint(event.to.x, event.to.y)
  if (!sourcePaneId) {
    return null
  }
  if (!destinationPaneId) {
    return null
  }
  if (validDestination(sourcePaneId, destinationPaneId)) {
    return null
  }
  return { sourcePaneId, destinationPaneId: null, isInvalid: true }
}

function dragOverState(event: DragOverEvent): TabDragState {
  const sourcePaneId = paneIdFromEntity(event.operation.source)
  const destinationPaneId = paneIdFromEntity(event.operation.target)
  const isInvalid =
    sourcePaneId !== null &&
    destinationPaneId !== null &&
    sourcePaneId !== destinationPaneId &&
    !validDestination(sourcePaneId, destinationPaneId)
  return { sourcePaneId, destinationPaneId: isInvalid ? null : destinationPaneId, isInvalid }
}

function useTabDragSession() {
  const moveTab = usePanesStore((state) => state.moveTab)
  const [dragState, setDragState] = useState<TabDragState>(IDLE_DRAG_STATE)
  const sourceTabId = useRef<string | null>(null)
  const sourceDomPosition = useRef<SourceDomPosition | null>(null)
  const restoreDragZoom = useRef<(() => void) | undefined>(undefined)

  const restoreSourceLabelFocus = () => {
    const tabId = sourceTabId.current
    if (tabId) {
      focusTabLabel(tabId)
    }
  }

  const handleMoveResult = (result: TabMoveResult) => {
    if (result.kind === 'transfer') {
      requestAnimationFrame(() => focusTabLabel(result.destinationTabId))
      return
    }
    if (result.kind === 'none') {
      restoreSourceLabelFocus()
    }
  }

  return {
    dragState,
    setDragState,
    start: (event: DragStartEvent) => {
      const sourcePaneId = paneIdFromEntity(event.operation.source)
      sourceTabId.current = sourceTabIdOf(event)
      const sourceElement = sourceTabId.current
        ? document.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(sourceTabId.current)}"]`)
        : null
      sourceDomPosition.current = domPositionOf(sourceElement)
      restoreDragZoom.current?.()
      // Let DnD capture the original placeholder and pointer origin first.
      queueMicrotask(() => {
        restoreDragZoom.current = sourceElement ? compensateDragZoom(sourceElement) : undefined
      })
      setDragState({ sourcePaneId, destinationPaneId: sourcePaneId, isInvalid: false })
    },
    finish: (drop: TabDrop | null) => {
      if (!drop) {
        restoreSourceLabelFocus()
        return
      }
      if (drop.sourcePaneId !== drop.destinationPaneId) {
        restoreDomPosition(sourceDomPosition.current)
      }
      const { sourcePaneId, destinationPaneId, destinationIndex } = drop
      void moveTab(sourcePaneId, drop.sourceTabId, destinationPaneId, destinationIndex).then(
        handleMoveResult,
      )
    },
    reset: () => {
      if (!isStillDragging(sourceDomPosition.current)) {
        restoreDragZoom.current?.()
      }
      restoreDragZoom.current = undefined
      sourceDomPosition.current = null
      setDragState(IDLE_DRAG_STATE)
    },
  }
}

export function TabDragDropProvider({ children }: TabDragDropProviderProps) {
  const tabs = useTabsStore((state) => state.panes)
  const session = useTabDragSession()
  const tabIds = useMemo(
    () => ({
      left: tabs.left.tabs.map((tab) => tab.id),
      right: tabs.right.tabs.map((tab) => tab.id),
    }),
    [tabs.left.tabs, tabs.right.tabs],
  )

  return (
    <DragDropProvider
      sensors={[mouseSensor]}
      onDragStart={session.start}
      onDragMove={(event) => {
        const invalidState = invalidMoveState(event)
        if (invalidState) {
          session.setDragState(invalidState)
        }
      }}
      onDragOver={(event) => {
        const nextState = dragOverState(event)
        session.setDragState(nextState)
        if (nextState.isInvalid) {
          event.preventDefault()
        }
      }}
      onDragEnd={(event) => {
        session.finish(resolveTabDrop(event, tabIds))
        session.reset()
      }}
    >
      <TabDragContext.Provider value={session.dragState}>{children}</TabDragContext.Provider>
    </DragDropProvider>
  )
}
