import { getDefaultAppErrorMessage } from '@/lib/default-app-errors'

const context = { appName: 'Sublime Text', extension: 'sql' }

describe('getDefaultAppErrorMessage', () => {
  it('maps default-application-rejected-dynamic-type with app and extension interpolated', () => {
    const message = getDefaultAppErrorMessage('default-application-rejected-dynamic-type', context)
    expect(message).toBe(
      "macOS won't allow “Sublime Text” to be set as the default for .sql files. macOS doesn't formally recognize this file type (common for types like .sql, .log, or .env).",
    )
  })

  it('maps default-application-write-failed with app and extension interpolated', () => {
    const message = getDefaultAppErrorMessage('default-application-write-failed', {
      appName: 'Visual Studio Code',
      extension: 'pdf',
    })
    expect(message).toBe(
      "Couldn't set “Visual Studio Code” as the default for .pdf files. Try a different app.",
    )
  })

  it('maps no-uti-for-extension with the extension interpolated', () => {
    const message = getDefaultAppErrorMessage('no-uti-for-extension', {
      appName: 'Sublime Text',
      extension: 'qwzzz',
    })
    expect(message).toBe(
      "macOS couldn't determine a file type for .qwzzz files, so a default can't be set.",
    )
  })

  it('maps app-missing-bundle-identifier with the app name interpolated', () => {
    const message = getDefaultAppErrorMessage('app-missing-bundle-identifier', context)
    expect(message).toBe("“Sublime Text” can't be used as a default application.")
  })

  it('maps app-info-plist-unreadable with the app name interpolated', () => {
    const message = getDefaultAppErrorMessage('app-info-plist-unreadable', {
      appName: 'AI Aggregator',
      extension: 'sql',
    })
    expect(message).toBe("“AI Aggregator” can't be used as a default application.")
  })

  it('falls back to a safe generic message for an unknown code', () => {
    expect(getDefaultAppErrorMessage('some-future-code', context)).toBe(
      "Couldn't set the default application. Please try a different app.",
    )
  })

  it('falls back to a safe generic message for unsupported', () => {
    expect(getDefaultAppErrorMessage('unsupported', context)).toBe(
      "Couldn't set the default application. Please try a different app.",
    )
  })

  it('falls back to a safe generic message for null/undefined codes', () => {
    expect(getDefaultAppErrorMessage(null, context)).toBe(
      "Couldn't set the default application. Please try a different app.",
    )
    expect(getDefaultAppErrorMessage(undefined, context)).toBe(
      "Couldn't set the default application. Please try a different app.",
    )
  })
})
