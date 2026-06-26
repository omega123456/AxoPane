/**
 * The single sanctioned logging sink for the frontend. Every other module must
 * route through these helpers — the `no-console` ESLint rule forbids raw
 * `console` usage everywhere except this file.
 *
 * Logs are forwarded to the Rust `log_frontend` command (via the low-level
 * `dispatch`, never `invokeCommand` — that logs every call and would recurse).
 * This keeps test runs quiet: the line goes to the mocked IPC sink instead of
 * the test console, and real diagnostics share the backend's log targets.
 * `console` is only used as a last-resort sink when IPC is unavailable (e.g.
 * before the Tauri runtime is ready).
 *
 * `debug` is gated to dev (`import.meta.env.DEV`) so production builds stay
 * quiet, while `info`/`warn`/`error` always emit so real problems surface in
 * any environment.
 */

import { dispatch } from '@/lib/ipc/dispatch'

type LogLevel = 'debug' | 'info' | 'warn' | 'error'
type LogContext = Record<string, unknown>

function isDev() {
  return Boolean(import.meta.env.DEV)
}

/** Serialize a log context to a string for the backend `details` field. */
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
    // Last-resort sink only — IPC unavailable or backend command not yet registered.
    const prefix = `[app:${level}] ${message}`
    if (context === undefined) {
      console[level](prefix, error)
    } else {
      console[level](prefix, context, error)
    }
  })
}

export const log = {
  /** Verbose tracing. Suppressed entirely outside dev builds. */
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

/**
 * Back-compatible info-level helper retained for existing call sites.
 */
export function logFrontend(message: string, context?: LogContext) {
  log.info(message, context)
}
