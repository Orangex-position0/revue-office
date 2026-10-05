import { useEffect, useState } from 'react'
import { Routes, Route, Navigate } from 'react-router-dom'
import { authApi } from '@/api'
import { useAuthStore } from '@/stores/auth-store'
import { AppLayout } from '@/components/layout/AppLayout'
import FilesPage from '@/pages/files/FilesPage'
import Studio from '@/pages/Studio'

let guestBootstrapPromise: Promise<void> | null = null

function getGuestDeviceId() {
  const key = 'revue-office:guest-device-id'
  const existing = window.localStorage.getItem(key)
  if (existing) return existing

  const deviceId =
    typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(36).slice(2)}`
  window.localStorage.setItem(key, deviceId)
  return deviceId
}

function App() {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
  const login = useAuthStore((s) => s.login)
  const [bootstrapping, setBootstrapping] = useState(!isAuthenticated)
  const [bootstrapError, setBootstrapError] = useState<string | null>(null)

  useEffect(() => {
    if (isAuthenticated) {
      setBootstrapping(false)
      return
    }

    setBootstrapping(true)
    setBootstrapError(null)
    guestBootstrapPromise ||= authApi
      .guestLogin(getGuestDeviceId())
      .then(({ data }) => {
        login(data.access_token, data.user)
      })
    guestBootstrapPromise
      .catch(() => {
        setBootstrapError('工作区准备失败，请检查后端服务后重试。')
      })
      .finally(() => {
        guestBootstrapPromise = null
        setBootstrapping(false)
      })
  }, [isAuthenticated, login])

  if (bootstrapping) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-surface-50 text-sm text-surface-600">
        正在准备工作区…
      </main>
    )
  }

  if (!isAuthenticated) {
    return (
      <main className="flex min-h-screen flex-col items-center justify-center gap-4 bg-surface-50 px-6 text-center text-surface-700">
        <p>{bootstrapError || '正在准备工作区…'}</p>
        {bootstrapError && (
          <button
            type="button"
            onClick={() => window.location.reload()}
            className="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
          >
            重试
          </button>
        )}
      </main>
    )
  }

  return (
    <Routes>
      <Route path="/login" element={<Navigate to="/" />} />
      <Route path="/" element={<Studio />} />
      <Route element={<AppLayout />}>
        <Route path="/files" element={<FilesPage />} />
        <Route path="/studio" element={<Navigate to="/" replace />} />
        <Route path="/*" element={<Navigate to="/" />} />
      </Route>
    </Routes>
  )
}

export default App
