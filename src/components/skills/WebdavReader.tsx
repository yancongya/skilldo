import { memo, useCallback, useState } from 'react'
import { ArrowLeft, Cloud, AlertCircle, CheckCircle } from 'lucide-react'
import type { TFunction } from 'i18next'

/** Minimal types matching the FullBackup JSON structure from Rust backup.rs */
type SkillTargetEntry = { tool: string; scope?: string; projectPath?: string } | string

type SkillEntry = {
  id: string
  name: string
  sourceType: string
  sourceRef?: string
  targets?: SkillTargetEntry[]
}

type FullBackup = {
  backupVersion?: number
  config?: {
    language?: string
    storagePath?: string
    gitCacheCleanupDays?: number
    gitCacheTtlSecs?: number
    githubToken?: string
    webdav?: { url?: string; user?: string; remoteDir?: string }
    exploreSources?: Array<{ id: string; name: string; sourceType: string; sourceRef: string }>
    toolDirOverrides?: Array<{ toolKey: string; path: string }>
    customScanDirs?: Array<{ path: string }>
  }
  skills?: SkillEntry[]
  exportedAt?: string
}

type WebdavReaderProps = {
  t: TFunction
}

function formatDate(epochStr: string): string {
  const secs = Number(epochStr)
  if (!Number.isFinite(secs) || secs <= 0) return epochStr
  return new Date(secs * 1000).toLocaleString()
}

function resolveTargetName(target: SkillTargetEntry): string {
  if (typeof target === 'string') return target
  return target.tool
}

const WebdavReader = memo(function WebdavReader({ t }: WebdavReaderProps) {
  const [url, setUrl] = useState('')
  const [user, setUser] = useState('')
  const [password, setPassword] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [backup, setBackup] = useState<FullBackup | null>(null)

  const handleConnect = useCallback(async () => {
    if (!url.trim()) return
    setLoading(true)
    setError(null)
    setBackup(null)
    try {
      const headers: Record<string, string> = {}
      if (user) {
        headers['Authorization'] = 'Basic ' + btoa(`${user}:${password}`)
      }
      const resp = await fetch(url, { headers })
      if (!resp.ok) {
        throw new Error(`HTTP ${resp.status} ${resp.statusText}`)
      }
      const text = await resp.text()
      const data = JSON.parse(text) as FullBackup
      setBackup(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [url, user, password])

  const handleClear = useCallback(() => {
    setBackup(null)
    setError(null)
  }, [])

  // ── Connection form ──
  if (!backup) {
    return (
      <div className="webdav-reader">
        <div className="webdav-reader-header">
          <Cloud size={20} />
          <span>{t('webdavReaderTitle')}</span>
        </div>
        <div className="webdav-reader-hint">{t('webdavReaderHint')}</div>
        <div className="webdav-reader-form">
          <label className="settings-field">
            <span>{t('webdavUrl')}</span>
            <input
              type="text"
              value={url}
              placeholder="https://dav.example.com/remote.php/dav/files/me/skilldo-backup.json"
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleConnect()}
            />
          </label>
          <div className="webdav-reader-auth-row">
            <label className="settings-field">
              <span>{t('webdavUser')}</span>
              <input type="text" value={user} onChange={(e) => setUser(e.target.value)} />
            </label>
            <label className="settings-field">
              <span>{t('webdavPassword')}</span>
              <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
            </label>
          </div>
          {error && (
            <div className="webdav-reader-error">
              <AlertCircle size={14} />
              <span>{error}</span>
            </div>
          )}
          <div className="settings-tool-dir-actions">
            <button
              className="btn btn-primary btn-sm"
              type="button"
              disabled={loading || !url.trim()}
              onClick={handleConnect}
            >
              {loading ? t('webdavConnecting') : t('webdavConnect')}
            </button>
          </div>
        </div>
      </div>
    )
  }

  // ── Backup data display ──
  const skills = backup.skills ?? []
  const config = backup.config
  const targetToolSet = new Set<string>()
  for (const skill of skills) {
    for (const target of skill.targets ?? []) {
      targetToolSet.add(resolveTargetName(target))
    }
  }

  return (
    <div className="webdav-reader">
      <div className="webdav-reader-header">
        <button className="btn btn-ghost btn-sm" type="button" onClick={handleClear}>
          <ArrowLeft size={16} />
        </button>
        <Cloud size={20} />
        <span>{t('webdavReaderTitle')}</span>
        <CheckCircle size={16} className="webdav-reader-ok" />
      </div>

      {/* Overview */}
      <div className="settings-section-divider" />
      <div className="settings-section-title">{t('webdavBackupOverview')}</div>
      <div className="webdav-reader-stats">
        <div className="webdav-reader-stat">
          <span className="webdav-reader-stat-value">{skills.length}</span>
          <span className="webdav-reader-stat-label">{t('webdavReaderSkills')}</span>
        </div>
        <div className="webdav-reader-stat">
          <span className="webdav-reader-stat-value">{targetToolSet.size}</span>
          <span className="webdav-reader-stat-label">{t('webdavReaderTools')}</span>
        </div>
        <div className="webdav-reader-stat">
          <span className="webdav-reader-stat-value">v{backup.backupVersion ?? '?'}</span>
          <span className="webdav-reader-stat-label">{t('webdavReaderVersion')}</span>
        </div>
        <div className="webdav-reader-stat">
          <span className="webdav-reader-stat-value">
            {backup.exportedAt ? formatDate(backup.exportedAt) : '—'}
          </span>
          <span className="webdav-reader-stat-label">{t('webdavReaderExportedAt')}</span>
        </div>
      </div>

      {/* Config */}
      {config && (
        <>
          <div className="settings-section-divider" />
          <div className="settings-section-title">{t('webdavReaderConfig')}</div>
          <div className="webdav-reader-config-grid">
            {config.storagePath && (
              <div className="webdav-reader-config-item">
                <span className="webdav-reader-config-key">{t('storagePath')}</span>
                <span className="mono">{config.storagePath}</span>
              </div>
            )}
            {config.language && (
              <div className="webdav-reader-config-item">
                <span className="webdav-reader-config-key">{t('language')}</span>
                <span>{config.language}</span>
              </div>
            )}
            {config.gitCacheCleanupDays != null && (
              <div className="webdav-reader-config-item">
                <span className="webdav-reader-config-key">{t('gitCacheCleanupDays')}</span>
                <span>{config.gitCacheCleanupDays}</span>
              </div>
            )}
            {config.gitCacheTtlSecs != null && (
              <div className="webdav-reader-config-item">
                <span className="webdav-reader-config-key">{t('gitCacheTtlSecs')}</span>
                <span>{config.gitCacheTtlSecs}s</span>
              </div>
            )}
            {config.exploreSources && config.exploreSources.length > 0 && (
              <div className="webdav-reader-config-item">
                <span className="webdav-reader-config-key">{t('webdavReaderSources')}</span>
                <span>{config.exploreSources.length}</span>
              </div>
            )}
          </div>
        </>
      )}

      {/* Skills list */}
      <div className="settings-section-divider" />
      <div className="settings-section-title">{t('webdavReaderSkillsList')}</div>
      {skills.length === 0 ? (
        <div className="settings-helper">{t('webdavReaderNoSkills')}</div>
      ) : (
        <div className="webdav-reader-skills">
          {skills.map((skill) => (
            <div className="webdav-reader-skill" key={skill.id}>
              <div className="webdav-reader-skill-name">{skill.name}</div>
              <div className="webdav-reader-skill-meta">
                <span className="webdav-reader-skill-type">{skill.sourceType}</span>
                {skill.sourceRef && (
                  <span className="mono webdav-reader-skill-ref">{skill.sourceRef}</span>
                )}
              </div>
              {skill.targets && skill.targets.length > 0 && (
                <div className="webdav-reader-skill-targets">
                  {skill.targets.map((target, i) => (
                    <span className="webdav-reader-skill-target" key={i}>
                      {resolveTargetName(target)}
                    </span>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
})

export default WebdavReader
