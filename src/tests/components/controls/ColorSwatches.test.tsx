import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ColorSwatches } from '@/components/controls'

describe('ColorSwatches', () => {
  it('updates from preset swatches and the color input', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()

    render(
      <ColorSwatches
        value="#3366ff"
        onChange={onChange}
        swatches={['#3366ff', '#ff7755', '#22aa88']}
      />,
    )

    await user.click(screen.getByRole('button', { name: 'Accent #ff7755' }))
    fireEvent.change(screen.getByLabelText('Custom accent color'), {
      target: { value: '#101010' },
    })

    expect(onChange).toHaveBeenNthCalledWith(1, '#ff7755')
    expect(onChange).toHaveBeenNthCalledWith(2, '#101010')
  })
})
