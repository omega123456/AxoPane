import { render } from '@testing-library/react'
import type { FileCategory } from '@/lib/file-type'
import { EntryIcon } from '@/components/icons/EntryIcon'

function svgFor(name: string, isDir = false) {
  const { container } = render(<EntryIcon entry={{ name, isDir }} />)
  return container.querySelector('svg')
}

describe('EntryIcon', () => {
  const cases: Array<[string, FileCategory]> = [
    ['main.ts', 'code'],
    ['index.html', 'web'],
    ['settings.json', 'config'],
    ['notes.txt', 'text'],
    ['report.pdf', 'office'],
    ['photo.png', 'image'],
    ['song.mp3', 'audio'],
    ['clip.mp4', 'video'],
    ['bundle.zip', 'archive'],
    ['disk.iso', 'disc'],
    ['installer.exe', 'executable'],
    ['font.ttf', 'font'],
    ['data.sqlite', 'database'],
    ['unknown.qwzzz', 'generic'],
  ]

  it.each(cases)('tags %s with data-file-category=%s', (name, category) => {
    const svg = svgFor(name)
    expect(svg).toHaveAttribute('data-file-category', category)
  })

  it('applies the default category color when none is overridden', () => {
    const svg = svgFor('main.ts')
    expect(svg).toHaveClass('text-file-code')
  })

  it('honors a colorClassName override', () => {
    const { container } = render(
      <EntryIcon entry={{ name: 'main.ts', isDir: false }} colorClassName="text-file-exec" />,
    )
    const svg = container.querySelector('svg')
    expect(svg).toHaveClass('text-file-exec')
    expect(svg).not.toHaveClass('text-file-code')
  })

  it('allows an empty colorClassName to inherit the surrounding color', () => {
    const { container } = render(
      <EntryIcon entry={{ name: 'main.ts', isDir: false }} colorClassName="" />,
    )
    const svg = container.querySelector('svg')
    expect(svg?.getAttribute('class')).not.toMatch(/text-/)
  })

  it('renders every folder with the two-tone folder glyph', () => {
    for (const name of ['Documents', 'Downloads', '.git', 'node_modules']) {
      const svg = svgFor(name, true)
      expect(svg).toHaveAttribute('data-file-category', 'folder')
      expect(svg?.querySelector('.fill-folder')).not.toBeNull()
      expect(svg?.querySelector('.fill-folder-back')).not.toBeNull()
    }
  })

  it('files render without a fill so they stay stroke-only', () => {
    const svg = svgFor('main.ts')
    expect(svg).not.toHaveAttribute('fill', 'currentColor')
  })

  it('renders a native icon image when the entry provides one', () => {
    const { container } = render(
      <EntryIcon
        entry={{
          name: 'installer.exe',
          isDir: false,
          iconDataUrl: 'data:image/png;base64,RkFLRQ==',
        }}
      />,
    )
    const image = container.querySelector('img')
    expect(image).toHaveAttribute('src', 'data:image/png;base64,RkFLRQ==')
    expect(container.querySelector('svg')).toBeNull()
  })

  it('adds a corner badge for a symlinked folder', () => {
    const { container } = render(
      <EntryIcon entry={{ name: 'linked_folder', isDir: true, attributes: ['symlink'] }} />,
    )
    expect(container.querySelector('[data-symlink="true"]')).not.toBeNull()
    expect(container.querySelector('[data-file-category="folder"]')).not.toBeNull()
    const badge = container.querySelectorAll('svg')[1]
    expect(badge?.getAttribute('class')).toMatch(/corner-up-right/)
  })

  it('adds a corner badge for a symlinked file', () => {
    const { container } = render(
      <EntryIcon entry={{ name: 'main.ts', isDir: false, attributes: ['symlink'] }} />,
    )
    expect(container.querySelector('[data-symlink="true"]')).not.toBeNull()
    expect(container.querySelectorAll('svg')).toHaveLength(2)
  })

  it('omits the badge for a plain folder without the symlink attribute', () => {
    const { container } = render(
      <EntryIcon entry={{ name: 'Documents', isDir: true, attributes: ['readonly'] }} />,
    )
    expect(container.querySelector('[data-symlink="true"]')).toBeNull()
    expect(container.querySelectorAll('svg')).toHaveLength(1)
  })
})
