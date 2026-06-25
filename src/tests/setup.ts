import '@testing-library/jest-dom/vitest'
import { beforeAll, beforeEach } from 'vitest'
import { ipc } from './ipc-mock'

class ResizeObserverMock {
  observe() {}

  unobserve() {}

  disconnect() {}
}

function matchMediaMock() {
  return {
    matches: false,
    media: '',
    onchange: null,
    addListener() {},
    removeListener() {},
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent() {
      return false
    },
  }
}

beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: matchMediaMock,
  })
  Object.defineProperty(window, 'ResizeObserver', {
    writable: true,
    value: ResizeObserverMock,
  })
  Element.prototype.scrollIntoView = () => {}
  HTMLElement.prototype.setPointerCapture = () => {}
  HTMLElement.prototype.releasePointerCapture = () => {}
})

beforeEach(() => {
  ipc.install()
})
