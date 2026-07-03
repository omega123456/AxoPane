import { render } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import App from '@/App'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { useQueueStore } from '@/stores/queue-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useTabsStore } from '@/stores/tabs-store'
import { initializeTheme } from '@/stores/theme-store'

export function renderApp() {
  useConfigStore.getState().reset()
  useLayoutStore.getState().reset()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  useSelectionStore.getState().reset()
  useQueueStore.getState().reset()
  initializeTheme()

  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  })

  return render(
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>,
  )
}
