import { lookupOwnProperty } from '@/lib/collections'

export interface DefaultAppErrorContext {
  appName: string
  extension: string
}

const GENERIC_FAILURE_MESSAGE = "Couldn't set the default application. Please try a different app."

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
