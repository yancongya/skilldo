import { memo, useCallback, useEffect, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { toast } from 'sonner'
import type { TFunction } from 'i18next'
import type {
  DevicePipelineReportDto,
  WebDavConfigDto,
} from '../types'

type BackupSyncSectionProps = {
  isTauri: boolean
  webdav: WebDavConfigDto | null
  onSaveWebdav: (webdav: WebDavConfigDto) => Promise<void>
  onBackupWebdav: () => Promise<void>
  onDevicePull: () => Promise<DevicePipelineReportDto>
  onDevicePublish: () => Promise<DevicePipelineReportDto>
  onDeviceStatus: () => Promise<DevicePipelineReportDto>
  onUpdateSkills: () => Promise<number>
  t: TFunction
}

const BackupSyncSection = memo(function BackupSyncSection({
  isTauri,
  webdav,
  onSaveWebdav,
  onBackupWebdav,
  onDevicePull,
  onDevicePublish,
  onDeviceStatus,
  onUpdateSkills,
  t,
}: BackupSyncSectionProps) {
  // backupMsg 已移除，使用 toast 反馈
  const [wdUrl, setWdUrl] = useState(webdav?.url ?? '')
  const [wdUser, setWdUser] = useState(webdav?.user ?? '')
  const [wdPassword, setWdPassword] = useState(webdav?.password ?? '')
  const [wdRemoteDir, setWdRemoteDir] = useState(webdav?.remoteDir ?? '')
  const [deviceReport, setDeviceReport] = useState<DevicePipelineReportDto | null>(null)
  const [deviceBusy, setDeviceBusy] = useState(false)
  const [webdavConfigOpen, setWebdavConfigOpen] = useState(false)
  const [lastBackupStatus, setLastBackupStatus] = useState<'pending' | 'success' | 'failed'>('pending')

  useEffect(() => {
    if (!isTauri) return
    void onDeviceStatus().then(setDeviceReport).catch(() => {})
  }, [isTauri, onDeviceStatus])

  useEffect(() => {
    setWdUrl(webdav?.url ?? '')
    setWdUser(webdav?.user ?? '')
    setWdPassword(webdav?.password ?? '')
    setWdRemoteDir(webdav?.remoteDir ?? '')
  }, [webdav])

  const handleSaveWebdav = useCallback(async () => {
    if (!isTauri) return
    try {
      await onSaveWebdav({
        url: wdUrl,
        user: wdUser,
        password: wdPassword,
        remoteDir: wdRemoteDir,
      })
      toast.success(t('saveWebdav'))
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err))
    }
  }, [isTauri, onSaveWebdav, wdUrl, wdUser, wdPassword, wdRemoteDir, t])

  const handleSync = useCallback(async () => {
    if (!isTauri) return
    setDeviceBusy(true)
    try {
      toast.info(t('syncUpdatingSkills'))
      const updatedCount = await onUpdateSkills()
      if (updatedCount > 0) toast.success(t('syncSkillsUpdated', { count: updatedCount }))
      toast.info(t('syncPulling'))
      const pullReport = await onDevicePull()
      setDeviceReport(pullReport)

      toast.info(t('syncPushing'))
      const pushReport = await onDevicePublish()
      setDeviceReport(pushReport)

      try {
        await onBackupWebdav()
        setLastBackupStatus('success')
        toast.success(t('syncDone'))
      } catch {
        setLastBackupStatus('failed')
        toast.warning(t('syncDoneNoBackup'))
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err))
    } finally {
      setDeviceBusy(false)
    }
  }, [isTauri, onUpdateSkills, onDevicePull, onDevicePublish, onBackupWebdav, t])

  return (
    <>
      <div className="settings-section-divider" />
      <div className="settings-section-title">{t('backupRestoreTitle')}</div>

      {/* WebDAV 连接（折叠） */}
      <button
        type="button"
        className="settings-section-subtitle settings-collapsible-header"
        onClick={() => setWebdavConfigOpen((v) => !v)}
        aria-expanded={webdavConfigOpen}
      >
        <span className="settings-collapsible-arrow">{webdavConfigOpen ? '▾' : '▸'}</span>
        {t('webdavConfig')}
        <span className={`settings-webdav-connected ${wdUrl ? '' : 'muted'}`}>
          {wdUrl ? t('webdavConnected') : t('webdavNotConfigured')}
        </span>
      </button>
      <div className="settings-helper settings-webdav-status">
        {wdUrl ? t(`webdavBackupStatus.${lastBackupStatus}`) : t('webdavNotConfigured')}
      </div>
      {webdavConfigOpen ? (
        <div className="settings-webdav-config">
          <div className="settings-webdav-grid">
            <label className="settings-field">
              <span>{t('webdavUrl')}</span>
              <input
                type="text"
                value={wdUrl}
                placeholder="https://dav.example.com/remote.php/dav/files/me"
                onChange={(e) => setWdUrl(e.target.value)}
              />
            </label>
            <label className="settings-field">
              <span>{t('webdavUser')}</span>
              <input type="text" value={wdUser} onChange={(e) => setWdUser(e.target.value)} />
            </label>
            <label className="settings-field">
              <span>{t('webdavPassword')}</span>
              <input
                type="password"
                value={wdPassword}
                onChange={(e) => setWdPassword(e.target.value)}
              />
            </label>
            <label className="settings-field">
              <span>{t('webdavRemoteDir')}</span>
              <input
                type="text"
                value={wdRemoteDir}
                placeholder="skilldo"
                onChange={(e) => setWdRemoteDir(e.target.value)}
              />
            </label>
          </div>
          <div className="settings-tool-dir-actions" style={{ marginTop: 12 }}>
            <button className="btn btn-secondary btn-sm" type="button" onClick={handleSaveWebdav}>
              {t('saveWebdav')}
            </button>
          </div>
        </div>
      ) : null}

      {/* 同步操作 */}
      <div className="settings-sync-buttons">
        <button
          className="btn btn-primary btn-sm"
          type="button"
          disabled={deviceBusy}
          onClick={handleSync}
        >
          {deviceBusy ? t('deviceWorking') : <><RefreshCw size={14} /> {t('syncNow')}</>}
        </button>
      </div>

      {/* 同步状态仪表板 */}
      {deviceReport ? (
        <div className="settings-sync-dashboard">
          <div className="settings-sync-stats">
            <div className="settings-sync-stat">
              <span className="settings-sync-stat-value">{deviceReport.state}</span>
              <span className="settings-sync-stat-label">{t('syncState')}</span>
            </div>
            <div className="settings-sync-stat">
              <span className="settings-sync-stat-value">{deviceReport.pushableRepositories}</span>
              <span className="settings-sync-stat-label">{t('syncPushable')}</span>
            </div>
            <div className="settings-sync-stat">
              <span className="settings-sync-stat-value">{deviceReport.dirtyRepositories}</span>
              <span className="settings-sync-stat-label">{t('syncDirty')}</span>
            </div>
            <div className="settings-sync-stat">
              <span className="settings-sync-stat-value">{deviceReport.pullableSkills}</span>
              <span className="settings-sync-stat-label">{t('syncPullable')}</span>
            </div>
            {deviceReport.failures.length > 0 && (
              <div className="settings-sync-stat settings-sync-stat-err">
                <span className="settings-sync-stat-value">{deviceReport.failures.length}</span>
                <span className="settings-sync-stat-label">{t('syncFailures')}</span>
              </div>
            )}
          </div>
          {deviceReport.stages.length > 0 && (
            <div className="settings-sync-stages">
              {deviceReport.stages.map((item) => (
                <div className="settings-sync-stage" key={item.id}>
                  <span className={`settings-sync-stage-dot ${item.status}`} />
                  <span>{item.message}</span>
                </div>
              ))}
            </div>
          )}
          {deviceReport.failures.map(([name, error]) => (
            <div className="settings-update-error" key={`${name}-${error}`}>
              <span>{name}</span><span>{error}</span>
            </div>
          ))}
        </div>
      ) : null}

    </>
  )
})

export default BackupSyncSection
