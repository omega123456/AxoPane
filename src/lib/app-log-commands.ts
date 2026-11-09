import { dispatch } from '@/lib/ipc/dispatch'
import type { FrontendLogLevel as LogLevel } from '@/lib/types/ipc'
type LogContext = Record<string, unknown>

function isDev() {
  return Boolean(import.meta.env.DEV)
}

function serializeContext(context?: LogContext): string | undefined {
  if (context === undefined) {
    return undefined
  }
  try {
    return JSON.stringify(context)
  } catch {
    return String(context)
  }
}

function emit(level: LogLevel, message: string, context?: LogContext) {
  void dispatch({
    command: 'log_frontend',
    payload: { level, message, category: 'frontend', details: serializeContext(context) },
  }).catch((error: unknown) => {
    const prefix = `[app:${level}] ${message}`
    if (context === undefined) {
      console[level](prefix, error)
      return
    }
    console[level](prefix, context, error)
  })
}

export const log = {
  debug(message: string, context?: LogContext) {
    if (!isDev()) {
      return
    }
    emit('debug', message, context)
  },
  info(message: string, context?: LogContext) {
    emit('info', message, context)
  },
  warn(message: string, context?: LogContext) {
    emit('warn', message, context)
  },
  error(message: string, context?: LogContext) {
    emit('error', message, context)
  },
}

export function logFrontend(message: string, context?: LogContext) {
  log.info(message, context)
}
