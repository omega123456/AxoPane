import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach } from 'vitest'
import { PropertiesDialog } from '@/components/dialogs/PropertiesDialog'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'

beforeEach(() => {
  usePropertiesDialogStore.getState().close()
})

describe('PropertiesDialog', () => {
  it('renders single-item properties and closes on button click', async () => {
    const user = userEvent.setup()
    usePropertiesDialogStore.getState().open({
      items: [
        {
          id: 'report',
          name: 'Report.txt',
          path: 'C:\\root\\Report.txt',
          isDir: false,
          sizeBytes: 2048,
          itemCount: null,
          typeLabel: 'TXT file',
          modifiedAt: '2026-06-20T10:15:00Z',
          createdAt: '2026-06-01T10:15:00Z',
          attributes: ['Archive'],
          isHidden: false,
          isSystem: false,
        },
      ],
    })

    render(<PropertiesDialog />)

    expect(screen.getByText('Report.txt')).toBeInTheDocument()
    expect(screen.getByText('2.0 KB')).toBeInTheDocument()
    expect(screen.getByText('TXT file')).toBeInTheDocument()
    expect(screen.getAllByText('—').length).toBeGreaterThan(0)

    await user.click(screen.getByRole('button', { name: 'Close' }))
    expect(usePropertiesDialogStore.getState().dialog).toBeNull()
  })

  it('renders multi-selection fallback content and closes on Escape', async () => {
    const user = userEvent.setup()
    usePropertiesDialogStore.getState().open({
      items: [
        {
          id: 'a',
          name: 'Alpha.txt',
          path: 'C:\\root\\Alpha.txt',
          isDir: false,
          sizeBytes: 12,
          itemCount: null,
          typeLabel: 'TXT file',
          modifiedAt: null,
          createdAt: null,
          attributes: [],
          isHidden: false,
          isSystem: false,
        },
        {
          id: 'b',
          name: 'Beta.zip',
          path: 'C:\\root\\Beta.zip',
          isDir: false,
          sizeBytes: 24,
          itemCount: null,
          typeLabel: 'ZIP archive',
          modifiedAt: null,
          createdAt: null,
          attributes: [],
          isHidden: false,
          isSystem: false,
        },
      ],
    })

    render(<PropertiesDialog />)

    expect(screen.getByText('2 items selected')).toBeInTheDocument()
    expect(screen.getByText('C:\\root\\Alpha.txt')).toBeInTheDocument()
    expect(screen.getByText('C:\\root\\Beta.zip')).toBeInTheDocument()

    await user.keyboard('{Escape}')
    expect(usePropertiesDialogStore.getState().dialog).toBeNull()
  })

  it('renders folder fallback details when native properties are unavailable', () => {
    usePropertiesDialogStore.getState().open({
      items: [
        {
          id: 'folder',
          name: 'Projects',
          path: 'C:\\root\\Projects',
          isDir: true,
          sizeBytes: null,
          itemCount: 5,
          typeLabel: 'Folder',
          modifiedAt: null,
          createdAt: null,
          attributes: [],
          isHidden: false,
          isSystem: false,
        },
      ],
    })

    render(<PropertiesDialog />)

    expect(screen.getByText('Projects')).toBeInTheDocument()
    expect(screen.getAllByText('5 items')).toHaveLength(2)
    expect(screen.getByText('Folder')).toBeInTheDocument()
  })
})
