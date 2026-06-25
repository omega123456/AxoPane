import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { RadioGroup } from '@/components/controls'

describe('RadioGroup', () => {
  it('renders radio semantics and changes the selected option', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()

    render(
      <RadioGroup
        ariaLabel="Copy conflicts"
        value="skip"
        onChange={onChange}
        options={[
          { value: 'skip', label: 'Skip' },
          { value: 'rename', label: 'Rename' },
        ]}
      />,
    )

    expect(screen.getByRole('radio', { name: 'Skip' })).toHaveAttribute('aria-checked', 'true')

    await user.click(screen.getByRole('radio', { name: 'Rename' }))

    expect(onChange).toHaveBeenCalledWith('rename')
  })
})
