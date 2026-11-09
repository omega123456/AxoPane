import { log } from '@/lib/app-log-commands'
import { getWindowBackdrop, setWindowTheme } from '@/lib/ipc/commands'
import type { ThemePreference } from '@/lib/types/ipc'

export async function syncNativeTheme(preference: ThemePreference) {
  try {
    await setWindowTheme({ theme: preference === 'system' ? null : preference })
  } catch (error) {
    log.warn('set_window_theme failed', { preference, error })
  }
}

export async function applyWindowBackdrop() {
  try {
    const backdrop = await getWindowBackdrop()
    document.documentElement.classList.toggle('mica', backdrop === 'mica')
  } catch (error) {
    log.warn('get_window_backdrop failed', { error })
  }
}
