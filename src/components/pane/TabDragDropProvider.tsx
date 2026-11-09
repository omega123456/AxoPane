import {
  createContext,
  type ReactNode,
  type RefObject,
  useContext,
  useMemo,
  useRef,
  useState,
} from 'react'
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
import { lookupOwnProperty } from '@/lib/collections'
import {
  focusTabLabel,
  paneIdAtPoint,
  paneIdFromEntity,
  resolveTabDrop,
  validDestination,
  type TabDragEndEvent,
  type TabDrop,
  type TabIdsByPane,
} from './tab-drop'

const TAB_DRAG_ACTIVATION_DISTANCE_PX = 5

const TAB_TYPE = 'tab'

const DND_DRAGGING_ATTRIBUTE = 'data-dnd-dragging'

const DND_PLACEHOLDER_SELECTOR = '[data-dnd-placeholder]'

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

type PointerEventHandler = (event: PointerEvent) => void

class MouseDistanceConstraint {
  private start: { x: number; y: number } | null = null
  private activateDrag: PointerEventHandler | null = null

  private readonly handlersByEventType: Partial<Record<string, PointerEventHandler>> = {
    pointerdown: (event) => this.recordStart(event),
    pointerup: () => this.abort(),
    pointermove: (event) => this.activateWhenFarEnough(event),
  }

  set controller(controller: { activate: PointerEventHandler }) {
    this.activateDrag = controller.activate.bind(controller)
  }

  onEvent(event: PointerEvent) {
    lookupOwnProperty(this.handlersByEventType, event.type)?.(event)
  }

  abort() {
    this.start = null
  }

  private recordStart(event: PointerEvent) {
    this.start = { x: event.clientX, y: event.clientY }
  }

  private activateWhenFarEnough(event: PointerEvent) {
    if (!this.start) {
      return
    }
    const distance = Math.hypot(event.clientX - this.start.x, event.clientY - this.start.y)
    if (distance >= TAB_DRAG_ACTIVATION_DISTANCE_PX) {
      this.activateDrag?.(event)
    }
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
    if (!element.hasAttribute(DND_DRAGGING_ATTRIBUTE)) restore()
  })
  const restore = () => {
    observer.disconnect()
    element.style.zoom = previousZoom
  }
  observer.observe(element, { attributes: true, attributeFilter: [DND_DRAGGING_ATTRIBUTE] })
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
  return position?.element.hasAttribute(DND_DRAGGING_ATTRIBUTE) === true
}

function restoreDomPosition(position: SourceDomPosition | null) {
  if (!position) return

  const placeholder = document.querySelector<HTMLElement>(DND_PLACEHOLDER_SELECTOR)
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

type TabDragRecord = {
  sourceTabId: string | null
  sourceDomPosition: SourceDomPosition | null
  restoreDragZoom?: () => void
}

const NO_TAB_DRAG_RECORD: TabDragRecord = { sourceTabId: null, sourceDomPosition: null }

function releasedDragRecord(record: TabDragRecord): TabDragRecord {
  return { sourceTabId: record.sourceTabId, sourceDomPosition: null }
}

function sourceTabElement(sourceTabId: string | null) {
  if (!sourceTabId) {
    return null
  }
  return document.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(sourceTabId)}"]`)
}

function restoreSourceLabelFocus(dragRecord: RefObject<TabDragRecord>) {
  const tabId = dragRecord.current.sourceTabId
  if (tabId) {
    focusTabLabel(tabId)
  }
}

function focusAfterMove(result: TabMoveResult, dragRecord: RefObject<TabDragRecord>) {
  if (result.kind === 'transfer') {
    requestAnimationFrame(() => focusTabLabel(result.destinationTabId))
    return
  }
  if (result.kind === 'none') {
    restoreSourceLabelFocus(dragRecord)
  }
}

function dragFeedbackHandlers(setDragState: (state: TabDragState) => void) {
  return {
    move: (event: DragMoveEvent) => {
      const invalidState = invalidMoveState(event)
      if (invalidState) {
        setDragState(invalidState)
      }
    },
    over: (event: DragOverEvent) => {
      const nextState = dragOverState(event)
      setDragState(nextState)
      if (nextState.isInvalid) {
        event.preventDefault()
      }
    },
  }
}

function useTabDragSession(tabIds: TabIdsByPane) {
  const moveTab = usePanesStore((state) => state.moveTab)
  const [dragState, setDragState] = useState<TabDragState>(IDLE_DRAG_STATE)
  const dragRecord = useRef<TabDragRecord>(NO_TAB_DRAG_RECORD)

  const finish = (drop: TabDrop | null) => {
    if (!drop) {
      restoreSourceLabelFocus(dragRecord)
      return
    }
    if (drop.sourcePaneId !== drop.destinationPaneId) {
      restoreDomPosition(dragRecord.current.sourceDomPosition)
    }
    const { sourcePaneId, sourceTabId, destinationPaneId, destinationIndex } = drop
    void moveTab({ sourcePaneId, tabId: sourceTabId, destinationPaneId, destinationIndex }).then(
      (result) => focusAfterMove(result, dragRecord),
    )
  }

  const reset = () => {
    const record = dragRecord.current
    if (!isStillDragging(record.sourceDomPosition)) {
      record.restoreDragZoom?.()
    }
    dragRecord.current = releasedDragRecord(record)
    setDragState(IDLE_DRAG_STATE)
  }

  return {
    dragState,
    start: (event: DragStartEvent) => {
      const sourcePaneId = paneIdFromEntity(event.operation.source)
      const sourceTabId = sourceTabIdOf(event)
      const sourceElement = sourceTabElement(sourceTabId)
      dragRecord.current.restoreDragZoom?.()
      dragRecord.current = { sourceTabId, sourceDomPosition: domPositionOf(sourceElement) }
      // Let DnD capture the original placeholder and pointer origin first.
      queueMicrotask(() => {
        const restoreDragZoom = sourceElement ? compensateDragZoom(sourceElement) : undefined
        dragRecord.current = { ...dragRecord.current, restoreDragZoom }
      })
      setDragState({ sourcePaneId, destinationPaneId: sourcePaneId, isInvalid: false })
    },
    ...dragFeedbackHandlers(setDragState),
    end: (event: TabDragEndEvent) => {
      finish(resolveTabDrop(event, tabIds))
      reset()
    },
  }
}

export function TabDragDropProvider({ children }: TabDragDropProviderProps) {
  const tabs = useTabsStore((state) => state.panes)
  const tabIds = useMemo(
    () => ({
      left: tabs.left.tabs.map((tab) => tab.id),
      right: tabs.right.tabs.map((tab) => tab.id),
    }),
    [tabs.left.tabs, tabs.right.tabs],
  )
  const session = useTabDragSession(tabIds)

  return (
    <DragDropProvider
      sensors={[mouseSensor]}
      onDragStart={session.start}
      onDragMove={session.move}
      onDragOver={session.over}
      onDragEnd={session.end}
    >
      <TabDragContext.Provider value={session.dragState}>{children}</TabDragContext.Provider>
    </DragDropProvider>
  )
}
