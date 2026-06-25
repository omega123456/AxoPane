/**
 * The single sanctioned logging sink for the frontend. Every other module must
 * route through these helpers — the `no-console` ESLint rule forbids raw
 * `console` usage everywhere except this file.
 *
 * `debug` is gated to dev (`import.meta.env.DEV`) so production builds stay
 * quiet, while `info`/`warn`/`error` always emit so real problems surface in
 * any environment.
 */

type LogContext = Record<string, unknown>

function isDev() {
  return Boolean(import.meta.env.DEV)
}

function emit(
  level: 'debug' | 'info' | 'warn' | 'error',
  message: string,
  context?: LogContext,
) {
  const prefix = `[app:${level}] ${message}`

  if (context === undefined) {
    console[level](prefix)
  } else {
    console[level](prefix, context)
  }
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
