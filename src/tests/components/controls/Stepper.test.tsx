import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { Stepper } from '@/components/controls'

describe('Stepper', () => {
  it('increments and decrements within bounds', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()

    render(
      <Stepper ariaLabel="Worker threads" value={4} min={2} max={8} step={2} onChange={onChange} />,
    )

    await user.click(screen.getByRole('button', { name: 'Increase' }))
    await user.click(screen.getByRole('button', { name: 'Decrease' }))

    expect(onChange).toHaveBeenNthCalledWith(1, 6)
    expect(onChange).toHaveBeenNthCalledWith(2, 2)
  })
})
