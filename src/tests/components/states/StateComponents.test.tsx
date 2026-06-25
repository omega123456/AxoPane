import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { vi } from 'vitest'
import { EmptyState } from '@/components/states/EmptyState'
import { ErrorState } from '@/components/states/ErrorState'
import { LoadingSkeleton } from '@/components/states/LoadingSkeleton'
import { PermissionDenied } from '@/components/states/PermissionDenied'

describe('state components', () => {
  it('renders the loading skeleton with a status role', () => {
    render(<LoadingSkeleton />)
    expect(screen.getByRole('status', { name: 'Loading folder' })).toBeInTheDocument()
  })

  it('renders the empty state message', () => {
    render(<EmptyState />)
    expect(screen.getByText('This folder is empty')).toBeInTheDocument()
  })

  it('renders the error state and fires retry / go-up', async () => {
    const user = userEvent.setup()
    const onRetry = vi.fn()
    const onGoUp = vi.fn()
    render(
      <ErrorState message="Boom" onRetry={onRetry} onGoUp={onGoUp} canGoUp />,
    )

    expect(screen.getByText('Boom')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Try again' }))
    await user.click(screen.getByRole('button', { name: 'Go up' }))

    expect(onRetry).toHaveBeenCalledOnce()
    expect(onGoUp).toHaveBeenCalledOnce()
  })

  it('disables go-up in the error state when at a root', () => {
    render(
      <ErrorState message="Boom" onRetry={vi.fn()} onGoUp={vi.fn()} canGoUp={false} />,
    )

    expect(screen.getByRole('button', { name: 'Go up' })).toBeDisabled()
  })

  it('renders permission denied with an OS-specific escape and go-up', async () => {
    const user = userEvent.setup()
    const onGoUp = vi.fn()
    render(<PermissionDenied onGoUp={onGoUp} canGoUp />)

    expect(screen.getByRole('alert', { name: 'Permission denied' })).toBeInTheDocument()
    expect(screen.getByText(/Open as Administrator|Open in Terminal/)).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Go up' }))
    expect(onGoUp).toHaveBeenCalledOnce()
  })
})
