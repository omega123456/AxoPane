import type { IpcCommandMap, NativeMenuItem } from '@/lib/types/ipc'

function item(
  overrides: Partial<NativeMenuItem> & Pick<NativeMenuItem, 'id' | 'label'>,
): NativeMenuItem {
  return {
    id: overrides.id,
    label: overrides.label,
    enabled: overrides.enabled ?? true,
    danger: overrides.danger ?? false,
    canonicalActionKind: overrides.canonicalActionKind ?? null,
    normalizedVerb: overrides.normalizedVerb ?? null,
    invokeToken: overrides.invokeToken ?? null,
    icon: overrides.icon ?? null,
    children: overrides.children ?? [],
  }
}

type LoadNativeMenuResponse = IpcCommandMap['load_native_menu']['response']

export const contextMenuFixtures: {
  nativeExtras: LoadNativeMenuResponse
  emptyNativeExtras: LoadNativeMenuResponse
  nativeFailureMessage: string
} = {
  nativeExtras: {
    requestId: 'fixture-native-request',
    items: [
      item({
        id: 'fixture-send-to-code',
        label: 'Open in VS Code',
        normalizedVerb: 'openinvscode',
        invokeToken: 'native:fixture-native-request:1',
        icon: {
          kind: 'dataUrl',
          dataUrl: 'data:image/png;base64,RkFLRQ==',
          alt: 'VS Code icon',
        },
      }),
      item({
        id: 'fixture-archive-tools',
        label: '7-Zip',
        children: [
          item({
            id: 'fixture-archive-open',
            label: 'Open archive',
            normalizedVerb: 'openarchive',
            invokeToken: 'native:fixture-native-request:2',
          }),
          item({
            id: 'fixture-archive-test',
            label: 'Test archive',
            normalizedVerb: 'testarchive',
            invokeToken: 'native:fixture-native-request:3',
          }),
        ],
      }),
      item({
        id: 'fixture-pin-to-quick-access',
        label: 'Pin to Quick access',
        normalizedVerb: 'pintoquickaccess',
        invokeToken: 'native:fixture-native-request:4',
      }),
    ],
  },
  emptyNativeExtras: {
    requestId: 'fixture-native-request',
    items: [],
  },
  nativeFailureMessage: 'Native shell extension lookup failed.',
}
