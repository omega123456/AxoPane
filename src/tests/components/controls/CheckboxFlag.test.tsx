import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { CheckboxFlag } from '@/components/controls'

describe('CheckboxFlag', () => {
  it('announces checkbox state and toggles on click', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()

    render(
      <CheckboxFlag
        checked={false}
        onChange={onChange}
        title="Watch network folders"
        description="Keep file watching enabled for mounted shares."
      />,
    )

    const checkbox = screen.getByRole('checkbox', { name: /Watch network folders/i })
    expect(checkbox).toHaveAttribute('aria-checked', 'false')

    await user.click(checkbox)

    expect(onChange).toHaveBeenCalledWith(true)
  })
})
