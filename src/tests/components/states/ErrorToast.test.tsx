import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { ErrorToast } from '@/components/states/ErrorToast'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useErrorToastStore } from '@/stores/error-toast-store'

beforeEach(() => {
  act(() => {
    useErrorToastStore.getState().dismiss()
    useActionDialogStore.getState().close()
  })
})

describe('ErrorToast', () => {
  it('renders nothing when there is no message', () => {
    const { container } = render(<ErrorToast />)
    expect(container).toBeEmptyDOMElement()
  })

  it('shows the message and dismisses on click', async () => {
    const user = userEvent.setup()
    render(<ErrorToast />)

    act(() => {
      useErrorToastStore.getState().show('restore failed: collision')
    })
    expect(screen.getByRole('alert')).toHaveTextContent('restore failed: collision')

    await user.click(screen.getByRole('button', { name: 'Dismiss error' }))
    expect(useErrorToastStore.getState().message).toBeNull()
  })

  it('routes a permission failure to the administrator restart prompt', () => {
    const { container } = render(<ErrorToast />)

    act(() => {
      useErrorToastStore.getState().show('Failed to rename: Access is denied. (os error 5)')
    })

    expect(container).toBeEmptyDOMElement()
    expect(useErrorToastStore.getState().message).toBeNull()
    expect(useActionDialogStore.getState().dialog).toEqual({
      kind: 'adminRequired',
      message: 'Failed to rename: Access is denied. (os error 5)',
    })
  })

  it('auto-dismisses after the timeout', () => {
    vi.useFakeTimers()
    render(<ErrorToast />)

    act(() => {
      useErrorToastStore.getState().show('boom')
    })
    expect(screen.getByRole('alert')).toBeInTheDocument()

    act(() => {
      vi.advanceTimersByTime(6_000)
    })
    expect(useErrorToastStore.getState().message).toBeNull()
    vi.useRealTimers()
  })

  it('replaces a pending auto-dismiss timer when shown again', () => {
    vi.useFakeTimers()

    act(() => {
      useErrorToastStore.getState().show('first')
    })
    act(() => {
      vi.advanceTimersByTime(3_000)
    })
    act(() => {
      useErrorToastStore.getState().show('second')
    })
    act(() => {
      vi.advanceTimersByTime(3_000)
    })
    expect(useErrorToastStore.getState().message).toBe('second')

    act(() => {
      vi.advanceTimersByTime(3_000)
    })
    expect(useErrorToastStore.getState().message).toBeNull()
    vi.useRealTimers()
  })
})
