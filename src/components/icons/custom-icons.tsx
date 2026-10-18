import type { ReactNode, SVGProps } from 'react'

type IconProps = SVGProps<SVGSVGElement>

function BaseIcon({ children, ...props }: IconProps & { children: ReactNode }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  )
}

export function SinglePaneIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="2" y="2" width="12" height="12" rx="1.5" />
    </BaseIcon>
  )
}

export function DualPaneIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="2" y="2" width="5.2" height="12" rx="1.5" />
      <rect x="8.8" y="2" width="5.2" height="12" rx="1.5" />
    </BaseIcon>
  )
}

/**
 * Filled two-tone folder, as in Windows 11 Explorer. The fills come from the
 * `folder` tokens, so the glyph ignores `currentColor` and reads the same in
 * both themes.
 */
export function FolderGlyph(props: IconProps) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" {...props}>
      <path
        className="fill-folder-back"
        d="M2.5 6.5a2 2 0 0 1 2-2h4.6l2 2h8.4a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-15a2 2 0 0 1-2-2z"
      />
      <path className="fill-folder" d="M2.5 9.5h19v8a2 2 0 0 1-2 2h-15a2 2 0 0 1-2-2z" />
    </svg>
  )
}
