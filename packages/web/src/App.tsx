import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { useCallback, useState } from 'react'
import { Dashboard } from './components/Dashboard'
import { SignIn } from './components/SignIn'
import { isUnauthorized } from './lib/api'
import { clearToken, loadToken, saveToken } from './lib/auth-storage'
import { disposeAllTerminals } from './lib/terminal-cache'

export function App() {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            // A rejected token will not work on a second try.
            retry: (failureCount, error) => !isUnauthorized(error) && failureCount < 1,
            refetchOnWindowFocus: true,
          },
        },
      }),
  )
  const [token, setToken] = useState<string | null>(loadToken)
  const [message, setMessage] = useState<string | null>(null)

  const signIn = (value: string) => {
    saveToken(value)
    setMessage(null)
    setToken(value)
  }

  const signOut = useCallback(() => {
    clearToken()
    queryClient.clear()
    disposeAllTerminals()
    setToken(null)
  }, [queryClient])

  const rejectToken = useCallback(() => {
    clearToken()
    queryClient.clear()
    disposeAllTerminals()
    setMessage('The daemon rejected that token. Paste the token it printed when it started.')
    setToken(null)
  }, [queryClient])

  return (
    <QueryClientProvider client={queryClient}>
      {token === null ? (
        <SignIn message={message} onSubmit={signIn} />
      ) : (
        <Dashboard token={token} onSignOut={signOut} onUnauthorized={rejectToken} />
      )}
    </QueryClientProvider>
  )
}
