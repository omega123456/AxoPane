import { detectPlatformOs } from '@/lib/keymap'

/**
 * Windows refuses filesystem writes outside the user's own folders unless the
 * process runs elevated. The OS reports this as `ERROR_ACCESS_DENIED` (5) or
 * `ERROR_PRIVILEGE_NOT_HELD` (1314), which reach the frontend as raw
 * `std::io::Error` text or as the matching COM `HRESULT`. The numeric codes
 * are the reliable part — the sentence around them is localized.
 */
const ADMIN_REQUIRED = /\(os error (?:5|1314)\)|0x80070005|0x80070522|access is denied/i

/**
 * Whether a failed operation looks like it needs administrator permissions.
 *
 * macOS has no equivalent relaunch, and its permission failures need a
 * different fix (Full Disk Access, ownership), so this stays Windows-only.
 */
export function needsAdminRestart(message: string | null | undefined): boolean {
  if (!message || detectPlatformOs() !== 'windows') {
    return false
  }

  return ADMIN_REQUIRED.test(message)
}
