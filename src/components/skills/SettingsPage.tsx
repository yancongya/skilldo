import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ArrowLeft, FolderOpen, RotateCcw } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { Update } from '@tauri-apps/plugin-updater'
import type {
  CustomScanDirEntry,
  ExploreSourceConfigDto,
  GithubOwnerEntry,
  GithubTokenStatusDto,
  OriginRules,
  ProfileSyncReportDto,
  DevicePipelineReportDto,
  RestoreReportDto,
  ToolDirOverride,
  ToolStatusDto,
  WebDavConfigDto,
} from './types'

type UpdateStatus = 'idle' | 'checking' | 'up-to-date' | 'available' | 'downloading' | 'done' | 'error'

type SettingsPageProps = {
  isTauri: boolean
  language: string
  storagePath: string
  gitCacheCleanupDays: number
  gitCacheTtlSecs: number
  githubToken: string
  originRules: OriginRules
  toolDirOverrides: ToolDirOverride[]
  onPickStoragePath: () => void
  onToggleLanguage: () => void
  onGitCacheCleanupDaysChange: (nextDays: number) => void
  onGitCacheTtlSecsChange: (nextSecs: number) => void
  onClearGitCacheNow: () => void
  onGithubTokenChange: (token: string) => void
  onOriginRulesChange: (rules: OriginRules) => void
  onSetToolDirOverride: (toolKey: string, path: string) => void
  onResetToolDirOverride: (toolKey: string) => void
  customScanDirs: CustomScanDirEntry[]
  onAddCustomScanDir: (path?: string) => void
  onRemoveCustomScanDir: (path: string) => void
  onBack: () => void
  t: TFunction
  exploreSources: ExploreSourceConfigDto[]
  onSaveExploreSources: (sources: ExploreSourceConfigDto[]) => void
  onExportConfig: () => Promise<void>
  onImportConfig: () => Promise<void>
  onValidateGithubToken: (token: string) => Promise<GithubTokenStatusDto>
  toolStatus: ToolStatusDto | null
  webdav: WebDavConfigDto | null
  onSaveWebdav: (webdav: WebDavConfigDto) => Promise<void>
  onBackupToFile: () => Promise<void>
  onRestoreFromFile: () => Promise<RestoreReportDto>
  onBackupWebdav: () => Promise<void>
  onRestoreWebdav: () => Promise<RestoreReportDto>
  onListGithubOwners: () => Promise<GithubOwnerEntry[]>
  onProfileStatus: () => Promise<ProfileSyncReportDto>
  onProfileSync: (applyDeletions: boolean) => Promise<ProfileSyncReportDto>
  onProfileExport: () => Promise<void>
  onProfileImport: (
    strategy: 'abort' | 'local' | 'remote',
  ) => Promise<ProfileSyncReportDto | null>
  onProfileResolve: (strategy: 'local' | 'remote') => Promise<ProfileSyncReportDto>
  onDeviceStatus: () => Promise<DevicePipelineReportDto>
  onDevicePull: () => Promise<DevicePipelineReportDto>
  onDevicePublish: () => Promise<DevicePipelineReportDto>
}

const SettingsPage = ({
  isTauri,
  language,
  storagePath,
  gitCacheCleanupDays,
  gitCacheTtlSecs,
  githubToken,
  originRules,
  toolDirOverrides,
  onPickStoragePath,
  onToggleLanguage,
  onGitCacheCleanupDaysChange,
  onGitCacheTtlSecsChange,
  onClearGitCacheNow,
  onGithubTokenChange,
  onOriginRulesChange,
  onSetToolDirOverride,
  onResetToolDirOverride,
  customScanDirs,
  onAddCustomScanDir,
  onRemoveCustomScanDir,
  onBack,
  t,
  exploreSources,
  onSaveExploreSources,
  onExportConfig,
  onImportConfig,
  onValidateGithubToken,
  toolStatus,
  webdav,
  onSaveWebdav,
  onBackupToFile,
  onRestoreFromFile,
  onBackupWebdav,
  onRestoreWebdav,
  onListGithubOwners,
  onProfileStatus,
  onProfileSync,
  onProfileExport,
  onProfileImport,
  onProfileResolve,
  onDeviceStatus,
  onDevicePull,
  onDevicePublish,
}: SettingsPageProps) => {
  const [localToken, setLocalToken] = useState(githubToken)
  useEffect(() => {
    setLocalToken(githubToken)
  }, [githubToken])

  const joinRules = (items: string[]) => items.join('\n')
  const parseRules = (value: string) =>
    value
      .split(/[,\n]/)
      .map((item) => item.trim())
      .filter(Boolean)

  const [officialGitReposText, setOfficialGitReposText] = useState(joinRules(originRules.officialGitRepos))

  useEffect(() => {
    setOfficialGitReposText(joinRules(originRules.officialGitRepos))
  }, [originRules])

  const handleSaveOriginRules = useCallback(() => {
    onOriginRulesChange({
      myGitOwners: originRules.myGitOwners,
      myGitRepos: originRules.myGitRepos,
      officialGitRepos: parseRules(officialGitReposText),
    })
  }, [originRules.myGitOwners, originRules.myGitRepos, officialGitReposText, onOriginRulesChange])

  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>('idle')
  const [updateVersion, setUpdateVersion] = useState<string | null>(null)
  const [updateError, setUpdateError] = useState<string | null>(null)
  const updateRef = useRef<Update | null>(null)

  const [manualDirInput, setManualDirInput] = useState('')
  const handleManualAddScanDir = useCallback(() => {
    const trimmed = manualDirInput.trim()
    if (trimmed) {
      onAddCustomScanDir(trimmed)
      setManualDirInput('')
    }
  }, [manualDirInput, onAddCustomScanDir])

  // ---- GitHub token validation ----
  const [tokenStatus, setTokenStatus] = useState<GithubTokenStatusDto | null>(null)
  const [validating, setValidating] = useState(false)
  const handleValidateToken = useCallback(async () => {
    if (!isTauri) return
    setValidating(true)
    setTokenStatus(null)
    try {
      const status = await onValidateGithubToken(localToken)
      setTokenStatus(status)
    } catch (err) {
      setTokenStatus({
        valid: false,
        scopes: [],
        error: err instanceof Error ? err.message : String(err),
      })
    } finally {
      setValidating(false)
    }
  }, [isTauri, localToken, onValidateGithubToken])

  // ---- Discovered GitHub owners ----
  const [discoveredOwners, setDiscoveredOwners] = useState<GithubOwnerEntry[]>([])
  const [loadingOwners, setLoadingOwners] = useState(false)
  const handleDiscoverOwners = useCallback(async () => {
    if (!isTauri) return
    setLoadingOwners(true)
    try {
      const owners = await onListGithubOwners()
      setDiscoveredOwners(owners)
    } catch {
      setDiscoveredOwners([])
    } finally {
      setLoadingOwners(false)
    }
  }, [isTauri, onListGithubOwners])
  const handleAddOwner = useCallback(
    (login: string) => {
      const next = { ...originRules, myGitOwners: [...new Set([...originRules.myGitOwners, login])] }
      onOriginRulesChange(next)
    },
    [originRules, onOriginRulesChange],
  )

  // ---- Skill sources management (unified config) ----
  const [sourceEditor, setSourceEditor] = useState<{
    source: ExploreSourceConfigDto
    isNew: boolean
  } | null>(null)
  const saveSources = useCallback(
    (next: ExploreSourceConfigDto[]) => {
      onSaveExploreSources(next)
    },
    [onSaveExploreSources],
  )
  const startAddSource = useCallback(() => {
    setSourceEditor({
      source: { id: '', name: '', kind: 'featured_json', endpoint: '', enabled: true, builtin: false },
      isNew: true,
    })
  }, [])
  const startEditSource = useCallback((s: ExploreSourceConfigDto) => {
    setSourceEditor({ source: { ...s }, isNew: false })
  }, [])
  const commitSource = useCallback(() => {
    if (!sourceEditor) return
    const draft = sourceEditor.source
    if (!draft.name.trim()) return
    const id =
      draft.id.trim() || `custom-${draft.name.trim().toLowerCase().replace(/\s+/g, '-')}`
    const next = sourceEditor.isNew
      ? [...exploreSources, { ...draft, id }]
      : exploreSources.map((x) => (x.id === id ? { ...draft, id } : x))
    saveSources(next)
    setSourceEditor(null)
  }, [sourceEditor, exploreSources, saveSources])
  const deleteSource = useCallback(
    (id: string) => {
      saveSources(exploreSources.filter((x) => x.id !== id))
    },
    [exploreSources, saveSources],
  )
  const toggleSource = useCallback(
    (id: string) => {
      saveSources(
        exploreSources.map((x) =>
          x.id === id ? { ...x, enabled: !x.enabled } : x,
        ),
      )
    },
    [exploreSources, saveSources],
  )

  // ---- Config backup ----
  const [backupMsg, setBackupMsg] = useState<string | null>(null)
  const handleExport = useCallback(async () => {
    if (!isTauri) return
    try {
      await onExportConfig()
      setBackupMsg(t('exportConfigDone'))
    } catch (err) {
      setBackupMsg(err instanceof Error ? err.message : String(err))
    }
  }, [isTauri, onExportConfig, t])
  const handleImport = useCallback(async () => {
    if (!isTauri) return
    try {
      if (!window.confirm(t('importConfigConfirm'))) return
      await onImportConfig()
      setBackupMsg(t('importConfigDone'))
    } catch (err) {
      setBackupMsg(
        t('importConfigFailed', {
          message: err instanceof Error ? err.message : String(err),
        }),
      )
    }
  }, [isTauri, onImportConfig, t])

  // ---- WebDAV backup ----
  const [wdUrl, setWdUrl] = useState(webdav?.url ?? '')
  const [wdUser, setWdUser] = useState(webdav?.user ?? '')
  const [wdPassword, setWdPassword] = useState(webdav?.password ?? '')
  const [wdRemoteDir, setWdRemoteDir] = useState(webdav?.remoteDir ?? '')
  const [restoreReport, setRestoreReport] = useState<RestoreReportDto | null>(null)
  const [profileReport, setProfileReport] = useState<ProfileSyncReportDto | null>(null)
  const [deviceReport, setDeviceReport] = useState<DevicePipelineReportDto | null>(null)
  const [deviceBusy, setDeviceBusy] = useState(false)

  const runDeviceAction = useCallback(
    async (mode: 'status' | 'pull' | 'publish') => {
      if (mode === 'publish' && !window.confirm(t('devicePublishConfirm'))) return
      setDeviceBusy(true)
      try {
        const report =
          mode === 'status'
            ? await onDeviceStatus()
            : mode === 'pull'
              ? await onDevicePull()
              : await onDevicePublish()
        setDeviceReport(report)
      } finally {
        setDeviceBusy(false)
      }
    },
    [onDevicePublish, onDevicePull, onDeviceStatus, t],
  )
  const [profileBusy, setProfileBusy] = useState(false)
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
      setBackupMsg(t('saveWebdav') + ' ✓')
    } catch (err) {
      setBackupMsg(err instanceof Error ? err.message : String(err))
    }
  }, [isTauri, onSaveWebdav, wdUrl, wdUser, wdPassword, wdRemoteDir, t])
  const handleBackupWebdav = useCallback(async () => {
    if (!isTauri) return
    try {
      await onBackupWebdav()
      setRestoreReport(null)
      setBackupMsg(t('backupDone'))
    } catch (err) {
      setBackupMsg(err instanceof Error ? err.message : String(err))
    }
  }, [isTauri, onBackupWebdav, t])
  const handleRestoreWebdav = useCallback(async () => {
    if (!isTauri) return
    try {
      const report = await onRestoreWebdav()
      setRestoreReport(report)
      setBackupMsg(t('restoreDone', { summary: report.summary }))
    } catch (err) {
      setBackupMsg(err instanceof Error ? err.message : String(err))
    }
  }, [isTauri, onRestoreWebdav, t])
  const handleBackupToFile = useCallback(async () => {
    if (!isTauri) return
    try {
      await onBackupToFile()
      setRestoreReport(null)
      setBackupMsg(t('backupDone'))
    } catch (err) {
      setBackupMsg(err instanceof Error ? err.message : String(err))
    }
  }, [isTauri, onBackupToFile, t])
  const handleRestoreFromFile = useCallback(async () => {
    if (!isTauri) return
    try {
      const report = await onRestoreFromFile()
      setRestoreReport(report)
      setBackupMsg(t('restoreDone', { summary: report.summary }))
    } catch (err) {
      setBackupMsg(err instanceof Error ? err.message : String(err))
    }
  }, [isTauri, onRestoreFromFile, t])

  const runProfileAction = useCallback(
    async (mode: 'status' | 'sync' | 'sync-delete') => {
      if (!isTauri) return
      if (mode === 'sync-delete' && !window.confirm(t('profileApplyDeletionsConfirm'))) return
      setProfileBusy(true)
      try {
        const report =
          mode === 'status' ? await onProfileStatus() : await onProfileSync(mode === 'sync-delete')
        setProfileReport(report)
        setBackupMsg(
          report.conflicts.length > 0
            ? t('profileConflictsFound', { count: report.conflicts.length })
            : t('profileSyncDone'),
        )
      } catch (error) {
        setBackupMsg(error instanceof Error ? error.message : String(error))
      } finally {
        setProfileBusy(false)
      }
    },
    [isTauri, onProfileStatus, onProfileSync, t],
  )

  const runProfileFileAction = useCallback(
    async (mode: 'export' | 'import') => {
      if (!isTauri) return
      setProfileBusy(true)
      try {
        if (mode === 'export') {
          await onProfileExport()
          setBackupMsg(t('profileExportDone'))
        } else {
          const report = await onProfileImport('abort')
          if (report) {
            setProfileReport(report)
            setBackupMsg(
              report.conflicts.length > 0
                ? t('profileConflictsFound', { count: report.conflicts.length })
                : t('profileImportDone'),
            )
          }
        }
      } catch (error) {
        setBackupMsg(error instanceof Error ? error.message : String(error))
      } finally {
        setProfileBusy(false)
      }
    },
    [isTauri, onProfileExport, onProfileImport, t],
  )

  const resolveProfile = useCallback(
    async (strategy: 'local' | 'remote') => {
      setProfileBusy(true)
      try {
        const report = await onProfileResolve(strategy)
        setProfileReport(report)
        setBackupMsg(t('profileResolved'))
      } catch (error) {
        setBackupMsg(error instanceof Error ? error.message : String(error))
      } finally {
        setProfileBusy(false)
      }
    },
    [onProfileResolve, t],
  )

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

  return (
    <div className="settings-page">
      <div className="detail-header">
        <button className="detail-back-btn" type="button" onClick={onBack}>
          <ArrowLeft size={16} />
          {t('detail.back')}
        </button>
        <div className="detail-skill-name">{t('settings')}</div>
      </div>
      <div className="settings-page-body">
        <div className="settings-field">
          <label className="settings-label" htmlFor="settings-language">
            {t('interfaceLanguage')}
          </label>
          <div className="settings-select-wrap">
            <select
              id="settings-language"
              className="settings-select"
              value={language}
              onChange={(event) => {
                if (event.target.value !== language) {
                  onToggleLanguage()
                }
              }}
            >
              <option value="en">{t('languageOptions.en')}</option>
              <option value="zh">{t('languageOptions.zh')}</option>
            </select>
            <svg
              className="settings-select-caret"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              aria-hidden="true"
            >
              <path d="M6 9l6 6 6-6" />
            </svg>
          </div>
        </div>

        <div className="settings-field">
          <label className="settings-label" htmlFor="settings-storage">
            {t('skillsStoragePath')}
          </label>
          <div className="settings-input-row">
            <input
              id="settings-storage"
              className="settings-input mono"
              value={storagePath}
              readOnly
            />
            <button
              className="btn btn-secondary settings-browse"
              type="button"
              onClick={onPickStoragePath}
            >
              {t('browse')}
            </button>
          </div>
          <div className="settings-helper">{t('skillsStorageHint')}</div>
        </div>

        <div className="settings-field">
          <label className="settings-label" htmlFor="settings-git-cache-days">
            {t('gitCacheCleanupDays')}
          </label>
          <div className="settings-input-row">
            <input
              id="settings-git-cache-days"
              className="settings-input"
              type="number"
              min={0}
              max={3650}
              step={1}
              value={gitCacheCleanupDays}
              onChange={(event) => {
                const next = Number(event.target.value)
                if (!Number.isNaN(next)) {
                  onGitCacheCleanupDaysChange(next)
                }
              }}
            />
            <button
              className="btn btn-secondary settings-browse"
              type="button"
              onClick={onClearGitCacheNow}
            >
              {t('cleanNow')}
            </button>
          </div>
          <div className="settings-helper">{t('gitCacheCleanupHint')}</div>
        </div>

        <div className="settings-field">
          <label className="settings-label" htmlFor="settings-git-cache-ttl">
            {t('gitCacheTtlSecs')}
          </label>
          <div className="settings-input-row">
            <input
              id="settings-git-cache-ttl"
              className="settings-input"
              type="number"
              min={0}
              max={3600}
              step={1}
              value={gitCacheTtlSecs}
              onChange={(event) => {
                const next = Number(event.target.value)
                if (!Number.isNaN(next)) {
                  onGitCacheTtlSecsChange(next)
                }
              }}
            />
          </div>
          <div className="settings-helper">{t('gitCacheTtlHint')}</div>
        </div>

        <div className="settings-field">
          <label className="settings-label" htmlFor="settings-github-token">
            {t('githubToken')}
          </label>
          <div className="settings-input-row">
            <input
              id="settings-github-token"
              className="settings-input mono"
              type="password"
              placeholder={t('githubTokenPlaceholder')}
              value={localToken}
              onChange={(e) => setLocalToken(e.target.value)}
              onBlur={() => {
                if (localToken !== githubToken) {
                  onGithubTokenChange(localToken)
                }
              }}
            />
            <button
              className="btn btn-secondary settings-browse"
              type="button"
              onClick={handleValidateToken}
              disabled={validating || !localToken.trim()}
            >
              {validating ? t('githubTokenValidating') : t('githubTokenValidate')}
            </button>
          </div>
          <div className="settings-helper">{t('githubTokenHint')}</div>
          {tokenStatus && (
            <div
              className={`settings-token-status ${tokenStatus.valid ? 'ok' : 'err'}`}
            >
              {tokenStatus.valid ? t('githubTokenValid') : t('githubTokenInvalid')}
              {tokenStatus.login && ` · ${t('githubTokenLogin')}: ${tokenStatus.login}`}
              {tokenStatus.scopes.length > 0 &&
                ` · ${t('githubTokenScopes')}: ${tokenStatus.scopes.join(', ')}`}
              {tokenStatus.error && ` · ${tokenStatus.error}`}
            </div>
          )}
        </div>

        <div className="settings-section-divider" />
        <div className="settings-section-title">{t('originRulesTitle')}</div>
        <div className="settings-helper" style={{ marginBottom: 12 }}>{t('originRulesHint')}</div>
        <div className="settings-field">
          <label className="settings-label" htmlFor="settings-origin-official-repos">
            {t('originRules.officialGitRepos')}
          </label>
          <textarea
            id="settings-origin-official-repos"
            className="settings-input settings-textarea mono"
            value={officialGitReposText}
            onChange={(event) => setOfficialGitReposText(event.target.value)}
            placeholder="openai/skills&#10;anthropics/skills"
          />
          <div className="settings-helper">{t('originRules.officialGitReposHint')}</div>
        </div>
        <button className="btn btn-primary btn-sm" type="button" onClick={handleSaveOriginRules}>
          {t('save')}
        </button>

        {/* Discover GitHub owners from repos */}
        {localToken ? (
          <>
            <div className="settings-field" style={{ marginTop: 16 }}>
              <label className="settings-label">{t('originRules.discoveredOwners')}</label>
              <div className="settings-helper" style={{ marginBottom: 8 }}>
                {t('originRules.discoveredOwnersHint')}
              </div>
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                onClick={handleDiscoverOwners}
                disabled={loadingOwners}
              >
                {loadingOwners ? '...' : t('discover')}
              </button>
              {discoveredOwners.length > 0 ? (
                <div className="github-owners-grid">
                  {discoveredOwners.map((owner) => {
                    const isAlreadyMine = originRules.myGitOwners.includes(owner.login)
                    return (
                      <div className="github-owner-card" key={owner.login}>
                        {owner.avatarUrl ? (
                          <img src={owner.avatarUrl} alt={owner.login} className="github-owner-avatar" />
                        ) : null}
                        <div className="github-owner-info">
                          <span className="github-owner-login">{owner.login}</span>
                          <span className="github-owner-repo-count">
                            {t('originRules.repoCount', { count: owner.repoCount })}
                          </span>
                        </div>
                        <button
                          className={`btn btn-sm ${isAlreadyMine ? 'btn-disabled' : 'btn-secondary'}`}
                          type="button"
                          onClick={() => handleAddOwner(owner.login)}
                          disabled={isAlreadyMine}
                        >
                          {isAlreadyMine ? '✓' : t('originRules.addAsMyOwner')}
                        </button>
                      </div>
                    )
                  })}
                </div>
              ) : null}
            </div>
          </>
        ) : (
          <div className="settings-helper" style={{ marginTop: 8, fontStyle: 'italic' }}>
            {t('originRules.noGithubToken')}
          </div>
        )}

        <div className="settings-section-divider" />
        <div className="settings-section-title">{t('manageSources')}</div>
        <div className="settings-helper" style={{ marginBottom: 12 }}>
          {t('manageSourcesHint')}
        </div>
        {exploreSources.map((src) => (
          <div className="settings-tool-dir-row" key={src.id}>
            <div className="settings-tool-dir-top">
              <span className="settings-tool-dir-label">
                {src.name}
                {src.builtin && (
                  <span className="settings-tool-dir-badge">{t('sourceKindFeatured')}</span>
                )}
              </span>
              <div className="settings-tool-dir-actions">
                <label className="settings-switch">
                  <input
                    type="checkbox"
                    checked={src.enabled}
                    onChange={() => toggleSource(src.id)}
                  />
                  <span>{t('sourceEnabled')}</span>
                </label>
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  onClick={() => startEditSource(src)}
                >
                  {t('editSource')}
                </button>
                {!src.builtin && (
                  <button
                    className="btn btn-secondary btn-sm"
                    type="button"
                    onClick={() => deleteSource(src.id)}
                  >
                    {t('deleteSource')}
                  </button>
                )}
              </div>
            </div>
            <div className="settings-tool-dir-path mono">{src.endpoint || src.kind}</div>
          </div>
        ))}
        <button
          className="btn btn-primary btn-sm"
          type="button"
          onClick={startAddSource}
          style={{ marginTop: 8 }}
        >
          {t('addSource')}
        </button>

        {sourceEditor && (
          <div className="settings-source-editor">
            <div className="settings-field">
              <label className="settings-label">{t('sourceName')}</label>
              <input
                className="settings-input"
                value={sourceEditor.source.name}
                onChange={(e) =>
                  setSourceEditor({
                    ...sourceEditor,
                    source: { ...sourceEditor.source, name: e.target.value },
                  })
                }
              />
            </div>
            <div className="settings-field">
              <label className="settings-label">{t('sourceKind')}</label>
              <div className="settings-select-wrap">
                <select
                  className="settings-select"
                  value={sourceEditor.source.kind}
                  onChange={(e) =>
                    setSourceEditor({
                      ...sourceEditor,
                      source: { ...sourceEditor.source, kind: e.target.value },
                    })
                  }
                >
                  <option value="featured_json">{t('sourceKindFeatured')}</option>
                  <option value="skills_sh">{t('sourceKindSkillsSh')}</option>
                  <option value="json_index">{t('sourceKindJsonIndex')}</option>
                  <option value="git_index">{t('sourceKindGitIndex')}</option>
                </select>
              </div>
            </div>
            <div className="settings-field">
              <label className="settings-label">{t('sourceEndpoint')}</label>
              <input
                className="settings-input mono"
                value={sourceEditor.source.endpoint}
                onChange={(e) =>
                  setSourceEditor({
                    ...sourceEditor,
                    source: { ...sourceEditor.source, endpoint: e.target.value },
                  })
                }
              />
            </div>
            <div className="settings-field">
              <label className="settings-label">{t('sourceEnabled')}</label>
              <input
                type="checkbox"
                checked={sourceEditor.source.enabled}
                onChange={(e) =>
                  setSourceEditor({
                    ...sourceEditor,
                    source: { ...sourceEditor.source, enabled: e.target.checked },
                  })
                }
              />
            </div>
            <div className="settings-tool-dir-actions">
              <button className="btn btn-primary btn-sm" type="button" onClick={commitSource}>
                {t('saveSource')}
              </button>
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                onClick={() => setSourceEditor(null)}
              >
                {t('cancel')}
              </button>
            </div>
          </div>
        )}

        <div className="settings-section-divider" />
        <div className="settings-section-title">{t('toolDirOverrideTitle')}</div>
        <div className="settings-helper" style={{ marginBottom: 12 }}>{t('toolDirOverrideHint')}</div>
        {toolDirOverrides.map((tdo) => (
          <ToolDirRow
            key={tdo.tool_key}
            tdo={tdo}
            isTauri={isTauri}
            onSet={onSetToolDirOverride}
            onReset={onResetToolDirOverride}
            t={t}
          />
        ))}

        <div className="settings-section-divider" />
        <div className="settings-section-title">{t('customScanDirs')}</div>
        <div className="settings-helper" style={{ marginBottom: 12 }}>{t('customScanDirHint')}</div>
        {customScanDirs.map((entry) => (
          <div className="settings-tool-dir-row" key={entry.path}>
            <div className="settings-tool-dir-top">
              <span className="settings-tool-dir-label">{entry.name}</span>
              <div className="settings-tool-dir-actions">
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  onClick={() => onRemoveCustomScanDir(entry.path)}
                >
                  {t('remove')}
                </button>
              </div>
            </div>
            <div className="settings-tool-dir-path mono">{entry.path}</div>
          </div>
        ))}
        <div className="settings-input-row" style={{ marginTop: 8 }}>
          <input
            className="settings-input mono"
            type="text"
            placeholder="~/path/to/skills"
            value={manualDirInput}
            onChange={(e) => setManualDirInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                handleManualAddScanDir()
              }
            }}
          />
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            onClick={handleManualAddScanDir}
            disabled={!manualDirInput.trim()}
          >
            {t('addScanDir')}
          </button>
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            onClick={() => onAddCustomScanDir()}
          >
            {t('browse')}
          </button>
        </div>

        <div className="settings-field settings-update-section">
          <label className="settings-label">{t('appUpdates')}</label>
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
              <span className="settings-update-status settings-update-ok">{t('updateNotAvailable')}</span>
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

        <div className="settings-section-divider" />
        <div className="settings-section-title">{t('statusOverview')}</div>
        <div className="settings-helper" style={{ marginBottom: 12 }}>
          {t('statusOverviewHint', { count: toolStatus?.tools.length ?? 0 })}
        </div>
        {toolStatus ? (
          <div className="settings-status-table">
            {toolStatus.tools.map((tool) => (
              <div className="settings-status-row" key={tool.key}>
                <span className="settings-status-name">{tool.label}</span>
                <span className={`settings-status-badge ${tool.installed ? 'ok' : 'muted'}`}>
                  {tool.installed ? t('statusInstalled') : t('statusNotInstalled')}
                  {toolStatus.newly_installed.includes(tool.key) && ` · ${t('statusNew')}`}
                </span>
                <span className="settings-status-path mono">{tool.skills_dir}</span>
              </div>
            ))}
          </div>
        ) : (
          <div className="settings-helper">{t('notAvailable')}</div>
        )}

        <div className="settings-section-divider" />
        <div className="settings-section-title">{t('webdav')}</div>
        <div className="settings-helper" style={{ marginBottom: 12 }}>
          {t('webdavHint')}
        </div>
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

        <div className="settings-section-divider" />
        <div className="settings-section-title">{t('profileSync')}</div>
        <div className="settings-helper" style={{ marginBottom: 12 }}>
          {t('profileSyncHint')}
        </div>
        <div className="settings-section-subtitle">{t('deviceSyncTitle')}</div>
        <div className="settings-helper">{t('deviceSyncHint')}</div>
        <div className="settings-tool-dir-actions">
          <button className="btn btn-secondary btn-sm" type="button" disabled={deviceBusy} onClick={() => runDeviceAction('status')}>
            {t('deviceStatus')}
          </button>
          <button className="btn btn-primary btn-sm" type="button" disabled={deviceBusy} onClick={() => runDeviceAction('pull')}>
            {deviceBusy ? t('deviceWorking') : t('devicePull')}
          </button>
          <button className="btn btn-primary btn-sm" type="button" disabled={deviceBusy} onClick={() => runDeviceAction('publish')}>
            {deviceBusy ? t('deviceWorking') : t('devicePublish')}
          </button>
        </div>
        {deviceReport ? (
          <div className="settings-restore-report" style={{ marginTop: 12 }}>
            <div className="settings-helper">
              {t('deviceSummary', {
                state: deviceReport.state,
                pushable: deviceReport.pushableRepositories,
                dirty: deviceReport.dirtyRepositories,
                pullable: deviceReport.pullableSkills,
                failures: deviceReport.failures.length,
              })}
            </div>
            {deviceReport.stages.map((item) => (
              <div className="settings-helper" key={item.id}>
                [{item.status}] {item.message}
              </div>
            ))}
            {deviceReport.failures.map(([name, error]) => (
              <div className="settings-update-error" key={`${name}-${error}`}>
                <span>{name}</span><span>{error}</span>
              </div>
            ))}
          </div>
        ) : null}
        <div className="settings-section-subtitle" style={{ marginTop: 16 }}>{t('profileAdvancedTitle')}</div>
        <div className="settings-tool-dir-actions">
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            disabled={profileBusy}
            onClick={() => runProfileAction('status')}
          >
            {t('profileStatus')}
          </button>
          <button
            className="btn btn-primary btn-sm"
            type="button"
            disabled={profileBusy}
            onClick={() => runProfileAction('sync')}
          >
            {profileBusy ? t('profileSyncing') : t('profileSyncNow')}
          </button>
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            disabled={profileBusy}
            onClick={() => runProfileFileAction('export')}
          >
            {t('profileExport')}
          </button>
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            disabled={profileBusy}
            onClick={() => runProfileFileAction('import')}
          >
            {t('profileImport')}
          </button>
          {profileReport && profileReport.pendingDeletions.length > 0 ? (
            <button
              className="btn btn-danger btn-sm"
              type="button"
              disabled={profileBusy}
              onClick={() => runProfileAction('sync-delete')}
            >
              {t('profileApplyDeletions', { count: profileReport.pendingDeletions.length })}
            </button>
          ) : null}
        </div>
        {profileReport ? (
          <div className="settings-restore-report" style={{ marginTop: 12 }}>
            <div className="settings-section-subtitle">
              {t('profileDevice')}: <span className="mono">{profileReport.deviceId}</span>
            </div>
            <div className="settings-helper">
              {t('profileSyncSummary', {
                installed: profileReport.installed.length,
                updated: profileReport.updated.length,
                conflicts: profileReport.conflicts.length,
                failures: profileReport.failures.length,
              })}
            </div>
            {profileReport.skippedLocal.length > 0 ? (
              <div className="settings-helper">
                {t('profileLocalSkipped', { count: profileReport.skippedLocal.length })}
              </div>
            ) : null}
            {profileReport.projectRepositories.length > 0 ? (
              <div className="settings-helper">
                {t('profileProjectRepositories', {
                  count: profileReport.projectRepositories.length,
                })}
              </div>
            ) : null}
            {profileReport.missingProjects.map((repository) => (
              <div className="settings-update-error" key={repository}>
                <span>{t('profileProjectMissing')}</span>
                <span className="mono">{repository}</span>
              </div>
            ))}
            {profileReport.conflicts.map((conflict) => (
              <div className="settings-update-error" key={conflict.path}>
                <span className="mono">{conflict.path}</span>
                <span>{conflict.reason}</span>
              </div>
            ))}
            {profileReport.conflicts.length > 0 && !profileReport.conflictsResolved ? (
              <div className="settings-tool-dir-actions" style={{ marginTop: 8 }}>
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  disabled={profileBusy}
                  onClick={() => resolveProfile('local')}
                >
                  {t('profileUseLocal')}
                </button>
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  disabled={profileBusy}
                  onClick={() => resolveProfile('remote')}
                >
                  {t('profileUseRemote')}
                </button>
              </div>
            ) : null}
            {profileReport.failures.map(([name, reason]) => (
              <div className="settings-update-error" key={`${name}:${reason}`}>
                <span>{name}</span>
                <span>{reason}</span>
              </div>
            ))}
          </div>
        ) : null}

        <div className="settings-section-divider" />
        <div className="settings-section-title">{t('configBackup')}</div>
        <div className="settings-helper" style={{ marginBottom: 12 }}>
          {t('configBackupHint')}
        </div>
        <div className="settings-tool-dir-actions">
          <button className="btn btn-secondary btn-sm" type="button" onClick={handleExport}>
            {t('exportConfig')}
          </button>
          <button className="btn btn-secondary btn-sm" type="button" onClick={handleImport}>
            {t('importConfig')}
          </button>
        </div>
        <div className="settings-helper" style={{ marginTop: 12 }}>
          {t('webdavHint')}
        </div>
        <div className="settings-tool-dir-actions" style={{ marginTop: 8 }}>
          <button className="btn btn-secondary btn-sm" type="button" onClick={handleBackupToFile}>
            {t('backupToFile')}
          </button>
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            onClick={handleRestoreFromFile}
          >
            {t('restoreFromFile')}
          </button>
          <button className="btn btn-secondary btn-sm" type="button" onClick={handleBackupWebdav}>
            {t('backupToWebdav')}
          </button>
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            onClick={handleRestoreWebdav}
          >
            {t('restoreFromWebdav')}
          </button>
        </div>
        {backupMsg && (
          <div className="settings-helper" style={{ marginTop: 8 }}>
            {backupMsg}
          </div>
        )}
        {restoreReport && (
          <div className="settings-restore-report" style={{ marginTop: 12 }}>
            <div className="settings-section-subtitle">{t('restoreReport')}</div>
            {restoreReport.installed.length > 0 && (
              <div className="settings-restore-group">
                <span className="settings-restore-tag ok">{t('restoreInstalled')}</span>
                <ul>
                  {restoreReport.installed.map((name) => (
                    <li key={name}>{name}</li>
                  ))}
                </ul>
              </div>
            )}
            {restoreReport.skipped.length > 0 && (
              <div className="settings-restore-group">
                <span className="settings-restore-tag muted">{t('restoreSkipped')}</span>
                <ul>
                  {restoreReport.skipped.map((item) => (
                    <li key={item.name}>
                      {item.name} — {item.reason}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {restoreReport.failed.length > 0 && (
              <div className="settings-restore-group">
                <span className="settings-restore-tag err">{t('restoreFailed')}</span>
                <ul>
                  {restoreReport.failed.map((item) => (
                    <li key={item.name}>
                      {item.name} — {item.reason}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}

      </div>
    </div>
  )
}

const ToolDirRow = memo(function ToolDirRow({
  tdo,
  isTauri,
  onSet,
  onReset,
  t,
}: {
  tdo: ToolDirOverride
  isTauri: boolean
  onSet: (toolKey: string, path: string) => void
  onReset: (toolKey: string) => void
  t: TFunction
}) {
  const [editing, setEditing] = useState(false)
  const [inputVal, setInputVal] = useState(tdo.current_dir)

  const handleRevealInFinder = useCallback(async () => {
    if (!isTauri) return
    try {
      const { revealItemInDir } = await import('@tauri-apps/plugin-opener')
      await revealItemInDir(tdo.current_dir)
    } catch {
      // ignore
    }
  }, [isTauri, tdo.current_dir])

  const handleSave = useCallback(() => {
    if (inputVal.trim()) {
      onSet(tdo.tool_key, inputVal.trim())
    }
    setEditing(false)
  }, [inputVal, tdo.tool_key, onSet])

  const handleCancel = useCallback(() => {
    setInputVal(tdo.current_dir)
    setEditing(false)
  }, [tdo.current_dir])

  return (
    <div className="settings-tool-dir-row">
      {editing ? (
        <div className="settings-tool-dir-edit-row">
          <input
            className="settings-input mono"
            value={inputVal}
            onChange={(e) => setInputVal(e.target.value)}
            placeholder={tdo.default_dir}
            autoFocus
          />
          <button className="btn btn-primary btn-sm" type="button" onClick={handleSave}>
            {t('save')}
          </button>
          <button className="btn btn-secondary btn-sm" type="button" onClick={handleCancel}>
            {t('cancel')}
          </button>
        </div>
      ) : (
        <>
          <div className="settings-tool-dir-top">
            <span className="settings-tool-dir-label">{tdo.label || tdo.tool_key}</span>
            {tdo.has_override && <span className="settings-tool-dir-badge">{t('customDir')}</span>}
            <div className="settings-tool-dir-actions">
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                onClick={handleRevealInFinder}
                title={t('openInFinder')}
              >
                <FolderOpen size={14} />
                <span>{t('preview')}</span>
              </button>
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                onClick={() => setEditing(true)}
              >
                {t('edit')}
              </button>
              {tdo.has_override && (
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  onClick={() => onReset(tdo.tool_key)}
                >
                  <RotateCcw size={14} />
                </button>
              )}
            </div>
          </div>
          <div className="settings-tool-dir-path mono">{tdo.current_dir}</div>
        </>
      )}
    </div>
  )
})

export default memo(SettingsPage)
