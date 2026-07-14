import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it } from 'vitest'
import { PaneViewMenu } from '@/components/pane/PaneViewMenu'
import { useTabsStore } from '@/stores/tabs-store'

describe('PaneViewMenu', () => {
  beforeEach(() => useTabsStore.getState().reset())

  it('labels the active mode and patches only the active tab', async () => {
    const user = userEvent.setup()
    useTabsStore
      .getState()
      .addTab('left', { path: 'C:\\two', sortKey: 'name', sortDirection: 'asc', filter: '' })
    render(<PaneViewMenu paneId="left" />)

    expect(screen.getByRole('button', { name: 'View: Details' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'View: Details' }))
    expect(screen.getByRole('menuitemradio', { name: 'Details' })).toHaveAttribute(
      'aria-checked',
      'true',
    )
    await user.click(screen.getByRole('menuitemradio', { name: 'Large thumbnails' }))

    const tabs = useTabsStore.getState().panes.left.tabs
    expect(tabs[0]?.viewMode).toBe('details')
    expect(tabs[1]?.viewMode).toBe('thumbnails')
  })
})
