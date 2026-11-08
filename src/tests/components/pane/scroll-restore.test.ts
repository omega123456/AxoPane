import { describe, expect, it } from 'vitest'
import { armedScrollRestore, scrollRestoreAfterAttempt } from '@/components/pane/scroll-restore'

const TALL = { scrollHeight: 1000, clientHeight: 100 }

describe('scroll restore', () => {
  it('settles once the target is reached or within a sub-pixel of it', () => {
    const restore = armedScrollRestore(200)
    expect(scrollRestoreAfterAttempt(restore, { ...TALL, scrollTop: 200 })).toBeNull()
    expect(scrollRestoreAfterAttempt(restore, { ...TALL, scrollTop: 199.5 })).toBeNull()
  })

  it('gives a fresh restore one retry even when it looks clamped', () => {
    const restore = armedScrollRestore(200)
    const retry = scrollRestoreAfterAttempt(restore, { ...TALL, scrollTop: 50 })
    expect(retry).toEqual({ target: 200, justArmed: false })
  })

  it('gives up on later attempts that are clamped or have no room to grow', () => {
    const retried = { target: 200, justArmed: false }
    expect(scrollRestoreAfterAttempt(retried, { ...TALL, scrollTop: 50 })).toBeNull()
    expect(
      scrollRestoreAfterAttempt(retried, { scrollTop: 50, scrollHeight: 100, clientHeight: 100 }),
    ).toBeNull()
  })
})
