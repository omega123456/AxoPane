import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { SelectField } from '@/components/controls'

describe('SelectField', () => {
  it('forwards native select changes', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()

    render(
      <SelectField
        ariaLabel="Sort sizes"
        value="bytes"
        onChange={onChange}
        options={[
          { value: 'bytes', label: 'Bytes' },
          { value: 'items', label: 'Items' },
        ]}
      />,
    )

    await user.selectOptions(screen.getByLabelText('Sort sizes'), 'items')

    expect(onChange).toHaveBeenCalledWith('items')
  })
})
