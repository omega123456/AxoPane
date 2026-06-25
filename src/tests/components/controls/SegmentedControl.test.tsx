import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { SegmentedControl } from '@/components/controls'

describe('SegmentedControl', () => {
  it('exposes radio semantics and updates the chosen segment', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()

    render(
      <SegmentedControl
        ariaLabel="Theme"
        value="system"
        onChange={onChange}
        options={[
          { value: 'system', label: 'System' },
          { value: 'light', label: 'Light' },
          { value: 'dark', label: 'Dark' },
        ]}
      />,
    )

    expect(screen.getByRole('radio', { name: 'System' })).toHaveAttribute('aria-checked', 'true')

    await user.click(screen.getByRole('radio', { name: 'Dark' }))

    expect(onChange).toHaveBeenCalledWith('dark')
  })
})
