import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { TFunction } from 'i18next'
import type { Update } from '@tauri-apps/plugin-updater'

type UpdateStatus = 'idle' | 'checking' | 'up-to-date' | 'available' | 'downloading' | 'done' | 'error'

type UpdateSectionProps = {
  isTauri: boolean
  t: TFunction
}

const UpdateSection = memo(function UpdateSection({
  isTauri,
  t,
}: UpdateSectionProps) {
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>('idle')
  const [updateVersion, setUpdateVersion] = useState<string | null>(null)
  const [updateError, setUpdateError] = useState<string | null>(null)
  const updateRef = useRef<Update | null>(null)

  const [appVersion, setAppVersion] = useState<string | null>(null)
  const versionText = useMemo(() => {
    if (!isTauri) return t('notAvailable')
    if (!appVersion) return t('unknown')
    return `v${appVersion}`
  }, [appVersion, isTauri, t])

  const loadAppVersion = useCallback(async () => {
    if (!isTauri) {
      setAppVersion(null)
      return
    }
    try {
      const { getVersion } = await import('@tauri-apps/api/app')
      const v = await getVersion()
      setAppVersion(v)
    } catch {
      setAppVersion(null)
    }
  }, [isTauri])

  useEffect(() => {
    void loadAppVersion()
    return () => { updateRef.current = null }
  }, [loadAppVersion])

  const handleCheckUpdate = useCallback(async () => {
    if (!isTauri) return
    setUpdateStatus('checking')
    setUpdateError(null)
    try {
      const { check } = await import('@tauri-apps/plugin-updater')
      const update = await check()
      if (update) {
        updateRef.current = update
        setUpdateVersion(update.version)
        setUpdateStatus('available')
      } else {
        setUpdateStatus('up-to-date')
      }
    } catch (err) {
      setUpdateError(err instanceof Error ? err.message : String(err))
      setUpdateStatus('error')
    }
  }, [isTauri])

  const handleInstallUpdate = useCallback(async () => {
    const update = updateRef.current
    if (!update) return
    setUpdateStatus('downloading')
    setUpdateError(null)
    try {
      await update.downloadAndInstall()
      setUpdateStatus('done')
    } catch (err) {
      setUpdateError(err instanceof Error ? err.message : String(err))
      setUpdateStatus('error')
    }
  }, [])

  const handleRestartUpdate = useCallback(async () => {
    try {
      const { relaunch } = await import('@tauri-apps/plugin-process')
      await relaunch()
    } catch (err) {
      setUpdateError(err instanceof Error ? err.message : String(err))
      setUpdateStatus('error')
    }
  }, [])

  return (
    <>
      <div className="settings-section-divider" />
      <div className="settings-section-title">{t('appUpdates')}</div>
      <div className="settings-field settings-update-section">
      <div className="settings-version-row">
        <span className="settings-version-text">
          {t('appName')} {versionText}
        </span>
        {isTauri && updateStatus === 'idle' && (
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            onClick={handleCheckUpdate}
          >
            {t('checkForUpdates')}
          </button>
        )}
        {updateStatus === 'checking' && (
          <span className="settings-update-status">{t('checkingUpdates')}</span>
        )}
        {updateStatus === 'up-to-date' && (
          <div className="settings-update-status settings-update-ok">
            <span>{t('updateNotAvailable')}</span>
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              onClick={handleCheckUpdate}
            >
              {t('checkForUpdates')}
            </button>
          </div>
        )}
      </div>
      {updateStatus === 'available' && (
        <div className="settings-update-available">
          <span>{t('updateAvailableWithVersion', { version: updateVersion })}</span>
          <button
            className="btn btn-primary btn-sm"
            type="button"
            onClick={handleInstallUpdate}
          >
            {t('downloadAndInstall')}
          </button>
        </div>
      )}
      {updateStatus === 'downloading' && (
        <div className="settings-update-status">{t('installingUpdate')}</div>
      )}
      {updateStatus === 'done' && (
        <div className="settings-update-available">
          <span className="settings-update-ok">{t('updateInstalledRestart')}</span>
          <button
            className="btn btn-primary btn-sm"
            type="button"
            onClick={handleRestartUpdate}
          >
            {t('restartNow')}
          </button>
        </div>
      )}
      {updateStatus === 'error' && (
        <div className="settings-update-error">
          <span>{updateError}</span>
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            onClick={handleCheckUpdate}
          >
            {t('checkForUpdates')}
          </button>
        </div>
      )}
      <div className="settings-helper">{t('updateHint')}</div>
      </div>
    </>
  )
})

export default UpdateSection