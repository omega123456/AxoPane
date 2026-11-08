import type { FormatDateOptions } from '@/lib/date-format'
import { useConfigStore } from '@/stores/config-store'

export function useDateDisplay(): FormatDateOptions {
  return {
    format: useConfigStore((state) => state.dateFormat),
    showTime: useConfigStore((state) => state.showTime),
    showSeconds: useConfigStore((state) => state.showSeconds),
    relative: useConfigStore((state) => state.relativeDates),
  }
}
