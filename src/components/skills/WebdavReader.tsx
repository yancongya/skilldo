import { memo, useCallback, useEffect, useState } from 'react'
import { Cloud, CheckCircle, RefreshCw } from 'lucide-react'
import type { TFunction } from 'i18next'
import { LOCAL_API_BASE } from './api'

type ApiSkill = {
  id: string
  name: string
  description?: string
  sourceType: string
  sourceRef?: string
  targets?: Array<{ tool: string; scope?: string; projectPath?: string }>
}

type ApiConfig = {
  language?: string
  storagePath?: string
  gitCacheCleanupDays?: number
  gitCacheTtlSecs?: number
  webdav?: { url?: string; user?: string; remoteDir?: string }
  exploreSources?: Array<{ id: string; name: string; sourceType: string; sourceRef: string }>
}

type WebdavReaderProps = {
  t: TFunction
}

const WebdavReader = memo(function WebdavReader({ t }: WebdavReaderProps) {
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [skills, setSkills] = useState<ApiSkill[]>([])
  const [config, setConfig] = useState<ApiConfig | null>(null)
  const [connected, setConnected] = useState(false)

  const [webdavUrl, setWebdavUrl] = useState('')
  const [webdavUser, setWebdavUser] = useState('')
  const [webdavPassword, setWebdavPassword] = useState('')

  const fetchFromApi = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [skillsResp, configResp] = await Promise.all([
        fetch(`${LOCAL_API_BASE}/api/skills`),
        fetch(`${LOCAL_API_BASE}/api/config`),
      ])
      if (!skillsResp.ok) throw new Error(`API ${skillsResp.status}`)
      const skillsData = await skillsResp.json()
      const configData = configResp.ok ? await configResp.json() : null
      setSkills(skillsData as ApiSkill[])
      setConfig(configData as ApiConfig | null)
      setConnected(true)
    } catch {
      setError(t('webdavApiUnavailable'))
      setConnected(false)
    } finally {
      setLoading(false)
    }
  }, [t])

  // Auto-fetch on mount
  useEffect(() => {
    void fetchFromApi()
  }, [fetchFromApi])

  const handleWebdavConnect = useCallback(async () => {
    if (!webdavUrl.trim()) return
    setLoading(true)
    setError(null)
    try {
      const headers: Record<string, string> = {}
      if (webdavUser) {
        headers['Authorization'] = 'Basic ' + btoa(`${webdavUser}:${webdavPassword}`)
      }
      const resp = await fetch(webdavUrl, { headers })
      if (!resp.ok) throw new Error(`HTTP ${resp.status} ${resp.statusText}`)
      const data = await resp.json()
      setSkills(data.skills ?? [])
      setConfig(data.config ?? null)
      setConnected(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [webdavUrl, webdavUser, webdavPassword])

  const targetToolSet = new Set<string>()
  for (const skill of skills) {
    for (const target of skill.targets ?? []) {
      targetToolSet.add(target.tool)
    }
  }

  // ── Loading state ──
  if (loading && !connected) {
    return (
      <div className="webdav-reader">
        <div className="webdav-reader-header">
          <Cloud size={20} />
          <span>{t('webdavReaderTitle')}</span>
        </div>
        <div className="settings-helper">{t('webdavConnecting')}</div>
      </div>
    )
  }

  // ── Not connected: show WebDAV fallback ──
  if (!connected) {
    return (
      <div className="webdav-reader">
        <div className="webdav-reader-header">
          <Cloud size={20} />
          <span>{t('webdavReaderTitle')}</span>
        </div>
        <div className="webdav-reader-hint">{error ?? t('webdavReaderHint')}</div>
        <div className="webdav-reader-form">
          <label className="settings-field">
            <span>{t('webdavUrl')}</span>
            <input
              type="text"
              value={webdavUrl}
              placeholder="https://dav.example.com/remote.php/dav/files/me/skilldo-backup.json"
              onChange={(e) => setWebdavUrl(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleWebdavConnect()}
            />
          </label>
          <div className="webdav-reader-auth-row">
            <label className="settings-field">
              <span>{t('webdavUser')}</span>
              <input type="text" value={webdavUser} onChange={(e) => setWebdavUser(e.target.value)} />
            </label>
            <label className="settings-field">
              <span>{t('webdavPassword')}</span>
              <input type="password" value={webdavPassword} onChange={(e) => setWebdavPassword(e.target.value)} />
            </label>
          </div>
          <div className="settings-tool-dir-actions">
            <button
              className="btn btn-primary btn-sm"
              type="button"
              disabled={loading || !webdavUrl.trim()}
              onClick={handleWebdavConnect}
            >
              {loading ? t('webdavConnecting') : t('webdavConnect')}
            </button>
          </div>
        </div>
      </div>
    )
  }

  // ── Connected: show skills list ──
  return (
    <div className="webdav-reader">
      <div className="webdav-reader-header">
        <Cloud size={20} />
        <span>{t('webdavReaderTitle')}</span>
        <CheckCircle size={16} className="webdav-reader-ok" />
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          onClick={() => void fetchFromApi()}
          title={t('webdavRefresh')}
          style={{ marginLeft: 'auto' }}
        >
          <RefreshCw size={14} />
        </button>
      </div>

      {/* Overview */}
      <div className="webdav-reader-stats">
        <div className="webdav-reader-stat">
          <span className="webdav-reader-stat-value">{skills.length}</span>
          <span className="webdav-reader-stat-label">{t('webdavReaderSkills')}</span>
        </div>
        <div className="webdav-reader-stat">
          <span className="webdav-reader-stat-value">{targetToolSet.size}</span>
          <span className="webdav-reader-stat-label">{t('webdavReaderTools')}</span>
        </div>
        {config?.storagePath && (
          <div className="webdav-reader-stat webdav-reader-stat-wide">
            <span className="webdav-reader-stat-label">{t('storagePath')}</span>
            <span className="mono webdav-reader-stat-value" style={{ fontSize: 13 }}>
              {config.storagePath}
            </span>
          </div>
        )}
      </div>

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
              {skill.description && (
                <div className="webdav-reader-skill-desc">{skill.description}</div>
              )}
              {skill.targets && skill.targets.length > 0 && (
                <div className="webdav-reader-skill-targets">
                  {skill.targets.map((target, i) => (
                    <span className="webdav-reader-skill-target" key={i}>
                      {target.tool}
                      {target.scope && target.scope !== 'global' ? ` (${target.scope})` : ''}
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
