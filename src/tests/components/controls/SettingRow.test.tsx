import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { SettingRow, ToggleSwitch } from '@/components/controls'

describe('SettingRow', () => {
  it('renders title, description, and trailing controls together', () => {
    render(
      <SettingRow
        title="Restore last session"
        description="Reopen the previous tabs and folders on launch."
        control={<ToggleSwitch checked={true} onChange={() => undefined} label="Restore last session" />}
      />,
    )

    expect(screen.getByText('Restore last session')).toBeInTheDocument()
    expect(screen.getByText('Reopen the previous tabs and folders on launch.')).toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'Restore last session' })).toBeInTheDocument()
  })
})
