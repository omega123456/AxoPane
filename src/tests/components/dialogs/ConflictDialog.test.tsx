import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ConflictDialog } from '@/components/dialogs/ConflictDialog'
import type { ConflictInfo } from '@/lib/types/ipc'

const conflict: ConflictInfo = {
  operationId: 'op-1',
  sourcePath: 'C:\\src\\photo.png',
  destinationPath: 'D:\\dst\\photo.png',
  name: 'photo.png',
}

const originalUserAgent = navigator.userAgent

function setUserAgent(value: string) {
  Object.defineProperty(navigator, 'userAgent', { value, configurable: true })
}

beforeEach(() => {
  setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')
})

afterEach(() => {
  setUserAgent(originalUserAgent)
})

describe('ConflictDialog', () => {
  it('defaults Enter to Skip', async () => {
    const onResolve = vi.fn()
    const user = userEvent.setup()
    render(<ConflictDialog conflict={conflict} onResolve={onResolve} />)

    await user.keyboard('{Enter}')
    expect(onResolve).toHaveBeenCalledWith('skip', false, null)
  })

  it('maps R to Replace, Esc to Skip', async () => {
    const onResolve = vi.fn()
    const user = userEvent.setup()
    render(<ConflictDialog conflict={conflict} onResolve={onResolve} />)

    await user.keyboard('r')
    expect(onResolve).toHaveBeenLastCalledWith('replace', false, null)

    await user.keyboard('{Escape}')
    expect(onResolve).toHaveBeenLastCalledWith('skip', false, null)
  })

  it('toggles apply-to-all with A and threads it through resolution', async () => {
    const onResolve = vi.fn()
    const user = userEvent.setup()
    render(<ConflictDialog conflict={conflict} onResolve={onResolve} />)

    await user.keyboard('a')
    await user.keyboard('r')
    expect(onResolve).toHaveBeenLastCalledWith('replace', true, null)
  })

  it('opens an inline rename input with N and submits the new name', async () => {
    const onResolve = vi.fn()
    const user = userEvent.setup()
    render(<ConflictDialog conflict={conflict} onResolve={onResolve} />)

    await user.keyboard('n')
    const input = await screen.findByRole('textbox', { name: 'New name' })
    await user.clear(input)
    await user.type(input, 'renamed.png')
    await user.keyboard('{Enter}')
    expect(onResolve).toHaveBeenCalledWith('rename', false, 'renamed.png')
  })

  it('escapes out of rename mode without resolving', async () => {
    const onResolve = vi.fn()
    const user = userEvent.setup()
    render(<ConflictDialog conflict={conflict} onResolve={onResolve} />)

    await user.keyboard('n')
    await screen.findByRole('textbox', { name: 'New name' })
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('textbox', { name: 'New name' })).not.toBeInTheDocument()
    expect(onResolve).not.toHaveBeenCalled()
  })

  it('orders Replace before Skip on Windows', () => {
    render(<ConflictDialog conflict={conflict} onResolve={vi.fn()} />)
    const buttons = screen.getAllByRole('button')
    const labels = buttons.map((button) => button.textContent ?? '')
    const replaceIndex = labels.findIndex((label) => label.startsWith('Replace'))
    const skipIndex = labels.findIndex((label) => label.startsWith('Skip'))
    expect(replaceIndex).toBeLessThan(skipIndex)
  })

  it('renders with macOS button order without crashing', () => {
    setUserAgent('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)')
    render(<ConflictDialog conflict={conflict} onResolve={vi.fn()} />)
    expect(screen.getByRole('dialog', { name: 'Resolve file conflict' })).toBeInTheDocument()
  })

  it('resolves via the backdrop click as a Skip', async () => {
    const onResolve = vi.fn()
    const user = userEvent.setup()
    render(<ConflictDialog conflict={conflict} onResolve={onResolve} />)
    await user.click(screen.getByRole('button', { name: 'Dismiss conflict' }))
    expect(onResolve).toHaveBeenCalledWith('skip', false, null)
  })

  it('uses a fixed viewport-wide wrapper so it covers the whole app, not just one pane', () => {
    render(<ConflictDialog conflict={conflict} onResolve={vi.fn()} />)
    expect(screen.getByRole('dialog', { name: 'Resolve file conflict' })).toHaveClass('fixed')
    expect(screen.getByRole('button', { name: 'Dismiss conflict' })).toHaveClass('absolute')
  })
})
