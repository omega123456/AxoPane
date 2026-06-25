import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ToggleSwitch } from '@/components/controls'

describe('ToggleSwitch', () => {
  it('renders switch semantics and flips the next value', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()

    render(<ToggleSwitch checked={false} onChange={onChange} label="Show hidden files" />)

    const toggle = screen.getByRole('switch', { name: 'Show hidden files' })
    expect(toggle).toHaveAttribute('aria-checked', 'false')

    await user.click(toggle)

    expect(onChange).toHaveBeenCalledWith(true)
  })
})
