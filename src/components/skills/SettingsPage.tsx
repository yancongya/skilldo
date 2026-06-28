import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ArrowLeft, FolderOpen, RotateCcw } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { Update } from '@tauri-apps/plugin-updater'
import type { CustomScanDirEntry, OriginRules, ToolDirOverride } from './types'

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
          </div>
          <div className="settings-helper">{t('githubTokenHint')}</div>
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
            <div className="settings-update-ok">{t('updateInstalledRestart')}</div>
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
