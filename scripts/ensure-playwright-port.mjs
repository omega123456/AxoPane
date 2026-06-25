const defaultPort = '4173'

if (!process.env.PLAYWRIGHT_PORT) {
  process.env.PLAYWRIGHT_PORT = defaultPort
}
