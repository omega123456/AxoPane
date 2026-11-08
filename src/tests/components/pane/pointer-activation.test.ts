import { describe, expect, it } from 'vitest'
import {
  entryPointerActivation,
  INITIAL_POINTER_ACTIVATION,
  parentPointerActivation,
} from '@/components/pane/pointer-activation'

const FIRST_CLICK_AT = 1_000
const SAME_BURST_AT = FIRST_CLICK_AT + 50
const LATER_CLICK_AT = FIRST_CLICK_AT + 300
const AFTER_PAIR_WINDOW_AT = FIRST_CLICK_AT + 900
const DOUBLE_CLICK = 2
const TRIPLE_CLICK = 3

describe('entryPointerActivation', () => {
  it('activates without touching state when the click carries no timing', () => {
    expect(entryPointerActivation(INITIAL_POINTER_ACTIVATION)).toEqual({
      shouldActivate: true,
      next: INITIAL_POINTER_ACTIVATION,
    })
    expect(entryPointerActivation(INITIAL_POINTER_ACTIVATION, DOUBLE_CLICK)).toEqual({
      shouldActivate: true,
      next: INITIAL_POINTER_ACTIVATION,
    })
  })

  it('records the activation time of a timed click', () => {
    const result = entryPointerActivation(INITIAL_POINTER_ACTIVATION, DOUBLE_CLICK, FIRST_CLICK_AT)
    expect(result.shouldActivate).toBe(true)
    expect(result.next.lastActivationAt).toBe(FIRST_CLICK_AT)
  })

  it('drops extra clicks from the same burst', () => {
    const afterFirst = { ...INITIAL_POINTER_ACTIVATION, lastActivationAt: FIRST_CLICK_AT }
    const result = entryPointerActivation(afterFirst, TRIPLE_CLICK, SAME_BURST_AT)
    expect(result).toEqual({ shouldActivate: false, next: afterFirst })
  })

  it('accepts extra clicks that arrive after the burst window', () => {
    const afterFirst = { ...INITIAL_POINTER_ACTIVATION, lastActivationAt: FIRST_CLICK_AT }
    expect(entryPointerActivation(afterFirst, TRIPLE_CLICK, LATER_CLICK_AT).shouldActivate).toBe(
      true,
    )
  })
})

describe('parentPointerActivation', () => {
  it('activates immediately when the click carries no timing', () => {
    expect(parentPointerActivation(INITIAL_POINTER_ACTIVATION).shouldActivate).toBe(true)
  })

  it('waits for a second click, then activates and clears the pending click', () => {
    const first = parentPointerActivation(INITIAL_POINTER_ACTIVATION, FIRST_CLICK_AT)
    expect(first).toEqual({
      shouldActivate: false,
      next: { lastActivationAt: null, parentPendingClickAt: FIRST_CLICK_AT },
    })
    const second = parentPointerActivation(first.next, LATER_CLICK_AT)
    expect(second).toEqual({
      shouldActivate: true,
      next: { lastActivationAt: LATER_CLICK_AT, parentPendingClickAt: null },
    })
  })

  it('restarts the pair when the second click is too late', () => {
    const pending = { lastActivationAt: null, parentPendingClickAt: FIRST_CLICK_AT }
    const result = parentPointerActivation(pending, AFTER_PAIR_WINDOW_AT)
    expect(result).toEqual({
      shouldActivate: false,
      next: { lastActivationAt: null, parentPendingClickAt: AFTER_PAIR_WINDOW_AT },
    })
  })

  it('ignores clicks from the burst that just activated something', () => {
    const justActivated = { lastActivationAt: FIRST_CLICK_AT, parentPendingClickAt: null }
    expect(parentPointerActivation(justActivated, SAME_BURST_AT)).toEqual({
      shouldActivate: false,
      next: justActivated,
    })
  })
})
