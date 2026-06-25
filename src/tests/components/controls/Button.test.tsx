import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { Button } from '@/components/controls'

describe('Button', () => {
  it('renders content and fires click handlers', async () => {
    const user = userEvent.setup()
    const onClick = vi.fn()

    render(
      <Button variant="primary" onClick={onClick}>
        Save changes
      </Button>,
    )

    await user.click(screen.getByRole('button', { name: 'Save changes' }))

    expect(onClick).toHaveBeenCalledTimes(1)
  })
})
