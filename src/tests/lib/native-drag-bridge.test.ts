import { afterEach, beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import {
  beginNativeDragBridge,
  endNativeDragBridge,
  subscribeNativeDragPositions,
} from '@/lib/native-drag-bridge'

const noModifiers = { ctrlKey: false, shiftKey: false, altKey: false, metaKey: false }

/**
 * jsdom has no layout, so `elementFromPoint` is scripted per test: each x maps to
 * the element standing at that spot.
 */
function hitTest(points: Record<number, Element | null>) {
  document.elementFromPoint = vi.fn((x: number) => points[x] ?? null)
}

function target(id: string) {
  const element = document.createElement('div')
  element.dataset.testid = id
  document.body.append(element)
  return element
}

function record(element: HTMLElement, events: string[]) {
  for (const type of ['dragenter', 'dragover', 'dragleave', 'drop']) {
    element.addEventListener(type, () => events.push(`${element.dataset.testid ?? ''}:${type}`))
  }
}

/**
 * Emits a cursor position that should land on viewport point (x, y). Rust reports
 * physical pixels relative to the window frame, so the frame is sized to the
 * viewport here — no chrome — and the point scaled by the device ratio.
 */
async function emitPosition(
  x: number,
  y: number,
  { chrome = 0, ...modifiers }: { chrome?: number } & Partial<typeof noModifiers> = {},
) {
  const ratio = window.devicePixelRatio || 1
  ipc.emit('drag://position', {
    cursorX: x * ratio,
    cursorY: (y + chrome) * ratio,
    frameWidth: window.innerWidth * ratio,
    frameHeight: (window.innerHeight + chrome) * ratio,
    ...noModifiers,
    ...modifiers,
  })
  // The bridge subscribes asynchronously; let that settle before asserting.
  await Promise.resolve()
}

let unsubscribe: Promise<() => void>

beforeEach(() => {
  ipc.install()
  document.body.innerHTML = ''
  endNativeDragBridge()
  unsubscribe = subscribeNativeDragPositions()
})

afterEach(async () => {
  ;(await unsubscribe)()
})

describe('native drag bridge', () => {
  it('replays cursor positions as drag events on the element under the pointer', async () => {
    const row = target('row')
    const events: string[] = []
    record(row, events)
    hitTest({ 10: row })

    beginNativeDragBridge()
    await emitPosition(10, 20)
    await emitPosition(10, 25)

    expect(events).toEqual(['row:dragenter', 'row:dragover', 'row:dragover'])
  })

  it('subtracts the window chrome so the pointer lands where the user sees it', async () => {
    const row = target('row')
    const events: string[] = []
    record(row, events)
    // Only viewport y=20 resolves to the row; a title bar left uncorrected would
    // shift every hit down by its height — one row, in practice.
    hitTest({ 10: row })
    const hits: number[] = []
    document.elementFromPoint = vi.fn((x: number, y: number) => {
      hits.push(y)
      return x === 10 && y === 20 ? row : null
    })

    beginNativeDragBridge()
    await emitPosition(10, 20, { chrome: 32 })

    expect(hits).toEqual([20])
    expect(events).toEqual(['row:dragenter', 'row:dragover'])
  })

  it('leaves the previous target when the pointer moves onto another one', async () => {
    const first = target('first')
    const second = target('second')
    const events: string[] = []
    record(first, events)
    record(second, events)
    hitTest({ 10: first, 50: second })

    beginNativeDragBridge()
    await emitPosition(10, 20)
    events.length = 0
    await emitPosition(50, 20)

    expect(events).toEqual(['first:dragleave', 'second:dragenter', 'second:dragover'])
  })

  it('treats a re-rendered row as the same target, not a boundary crossing', async () => {
    const row = target('row')
    row.dataset.entryId = 'Alpha'
    const events: string[] = []
    record(row, events)
    // A re-render swaps the row's DOM node; the old one is left detached, so
    // element identity (and `contains`) can no longer recognise it.
    const rerendered = target('row')
    rerendered.dataset.entryId = 'Alpha'
    record(rerendered, events)
    hitTest({ 10: row, 11: rerendered })

    beginNativeDragBridge()
    await emitPosition(10, 20)
    row.remove()
    events.length = 0
    await emitPosition(11, 20)

    expect(events).toEqual(['row:dragover'])
  })

  it('does not churn leave/enter when the hit-test flips between a row and its child', async () => {
    const row = target('row')
    const label = document.createElement('span')
    row.append(label)
    const events: string[] = []
    record(row, events)
    hitTest({ 10: row, 11: label })

    beginNativeDragBridge()
    await emitPosition(10, 20)
    events.length = 0
    // Sub-pixel jitter over the same row resolves to the row, then its text span.
    await emitPosition(11, 20)
    await emitPosition(10, 20)

    // Only dragover — a leave/enter pair here clears and re-sets the drop
    // highlight, which reads as flicker at 60 Hz.
    expect(events).toEqual(['row:dragover', 'row:dragover'])
  })

  it('names the element on the other side of a real boundary crossing', async () => {
    const first = target('first')
    const second = target('second')
    const related: (EventTarget | null)[] = []
    first.addEventListener('dragleave', (event) => {
      related.push((event as MouseEvent).relatedTarget)
    })
    second.addEventListener('dragenter', (event) => {
      related.push((event as MouseEvent).relatedTarget)
    })
    hitTest({ 10: first, 50: second })

    beginNativeDragBridge()
    await emitPosition(10, 20)
    await emitPosition(50, 20)

    // Drop handlers use relatedTarget to tell a real exit from an internal move.
    expect(related).toEqual([second, first])
  })

  it('drops on the last hovered target when the OS session ends', async () => {
    const row = target('row')
    const events: string[] = []
    record(row, events)
    hitTest({ 10: row })

    beginNativeDragBridge()
    await emitPosition(10, 20)
    events.length = 0
    endNativeDragBridge()

    expect(events).toEqual(['row:drop'])
  })

  it('drops nothing when the drag ends outside the window', async () => {
    const row = target('row')
    const events: string[] = []
    record(row, events)
    hitTest({ 10: row, 999: null })

    beginNativeDragBridge()
    await emitPosition(10, 20)
    // Pointer left the webview — this is a drop into another application.
    await emitPosition(999, 20)
    events.length = 0
    endNativeDragBridge()

    expect(events).toEqual([])
  })

  it('ignores positions once the drag is over', async () => {
    const row = target('row')
    const events: string[] = []
    record(row, events)
    hitTest({ 10: row })

    beginNativeDragBridge()
    endNativeDragBridge()
    await emitPosition(10, 20)

    expect(events).toEqual([])
  })

  it('tracks modifiers pressed mid-drag, which the webview never sees as key events', async () => {
    const row = target('row')
    const seen: boolean[] = []
    row.addEventListener('dragover', (event) => {
      seen.push((event as MouseEvent).metaKey)
    })
    hitTest({ 10: row })

    beginNativeDragBridge()
    await emitPosition(10, 20)
    // Cmd goes down partway through the gesture.
    await emitPosition(10, 21, { metaKey: true })
    await emitPosition(10, 22)

    expect(seen).toEqual([false, true, false])
  })

  it('drops with the modifiers held at the moment of the drop', async () => {
    const row = target('row')
    let dropMeta: boolean | undefined
    row.addEventListener('drop', (event) => {
      dropMeta = (event as MouseEvent).metaKey
    })
    hitTest({ 10: row })

    beginNativeDragBridge()
    await emitPosition(10, 20)
    await emitPosition(10, 21, { metaKey: true })
    endNativeDragBridge()

    // Copy-vs-move is decided at drop time, so the last known state is the one
    // that counts.
    expect(dropMeta).toBe(true)
  })

  it('exposes a dataTransfer that drop targets can set a dropEffect on', async () => {
    const row = target('row')
    let dropEffect: string | undefined
    row.addEventListener('dragover', (event) => {
      const transfer = (event as DragEvent).dataTransfer
      if (transfer) {
        transfer.dropEffect = 'move'
        dropEffect = transfer.dropEffect
      }
    })
    hitTest({ 10: row })

    beginNativeDragBridge()
    await emitPosition(10, 20)

    expect(dropEffect).toBe('move')
  })
})
