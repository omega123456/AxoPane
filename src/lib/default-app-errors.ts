/**
 * Pure mapping from a backend `set_default_application` status code to
 * honest, user-facing inline-error copy.
 *
 * This module is intentionally dependency-free (no React, no IPC) so it can
 * be reused by `DefaultAppDialog` and trivially unit-tested. The backend
 * (see `src-tauri/src/app_picker/macos.rs`) owns the stable machine codes;
 * this module owns the interpolated human copy.
 */

import { lookupOwnProperty } from '@/lib/collections'

/** Context used to interpolate `{app}` / `{ext}` placeholders in copy. */
export interface DefaultAppErrorContext {
  /** Display name of the application the user selected, e.g. "Sublime Text". */
  appName: string
  /** File extension without the leading dot, e.g. "sql". */
  extension: string
}

const GENERIC_FAILURE_MESSAGE = "Couldn't set the default application. Please try a different app."

/**
 * Maps a backend status `message` code to user-facing inline-error copy.
 *
 * Unknown/missing codes (including `unsupported` and IPC-error fallbacks)
 * resolve to a safe, generic message so the dialog never renders a raw
 * machine code to the user.
 */
type DefaultAppErrorCopy = (context: DefaultAppErrorContext) => string

const unusableAppMessage: DefaultAppErrorCopy = ({ appName }) =>
  `“${appName}” can't be used as a default application.`

const defaultAppErrorMessages: Readonly<Record<string, DefaultAppErrorCopy>> = {
  'default-application-rejected-dynamic-type': ({ appName, extension }) =>
    `macOS won't allow “${appName}” to be set as the default for .${extension} files. macOS doesn't formally recognize this file type (common for types like .sql, .log, or .env).`,
  'default-application-write-failed': ({ appName, extension }) =>
    `Couldn't set “${appName}” as the default for .${extension} files. Try a different app.`,
  'no-uti-for-extension': ({ extension }) =>
    `macOS couldn't determine a file type for .${extension} files, so a default can't be set.`,
  'app-missing-bundle-identifier': unusableAppMessage,
  'app-info-plist-unreadable': unusableAppMessage,
}

export function getDefaultAppErrorMessage(
  code: string | null | undefined,
  context: DefaultAppErrorContext,
): string {
  const message = code ? lookupOwnProperty(defaultAppErrorMessages, code) : undefined
  return message ? message(context) : GENERIC_FAILURE_MESSAGE
}
