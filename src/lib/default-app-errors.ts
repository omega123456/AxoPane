/**
 * Pure mapping from a backend `set_default_application` status code to
 * honest, user-facing inline-error copy.
 *
 * This module is intentionally dependency-free (no React, no IPC) so it can
 * be reused by `DefaultAppDialog` and trivially unit-tested. The backend
 * (see `src-tauri/src/app_picker/macos.rs`) owns the stable machine codes;
 * this module owns the interpolated human copy.
 */

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
export function getDefaultAppErrorMessage(
  code: string | null | undefined,
  context: DefaultAppErrorContext,
): string {
  const { appName, extension } = context

  switch (code) {
    case 'default-application-rejected-dynamic-type':
      return `macOS won't allow “${appName}” to be set as the default for .${extension} files. macOS doesn't formally recognize this file type (common for types like .sql, .log, or .env).`
    case 'default-application-write-failed':
      return `Couldn't set “${appName}” as the default for .${extension} files. Try a different app.`
    case 'no-uti-for-extension':
      return `macOS couldn't determine a file type for .${extension} files, so a default can't be set.`
    case 'app-missing-bundle-identifier':
    case 'app-info-plist-unreadable':
      return `“${appName}” can't be used as a default application.`
    default:
      return GENERIC_FAILURE_MESSAGE
  }
}
