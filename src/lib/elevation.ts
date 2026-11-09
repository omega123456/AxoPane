import { isWindowsPlatform } from '@/lib/keymap'

const ADMIN_REQUIRED = /\(os error (?:5|1314)\)|0x80070005|0x80070522|access is denied/i

export function needsAdminRestart(message: string | null | undefined): boolean {
  if (!message) {
    return false
  }
  if (!isWindowsPlatform()) {
    return false
  }

  return ADMIN_REQUIRED.test(message)
}
