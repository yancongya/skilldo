import { memo } from 'react'
import { ArrowLeft } from 'lucide-react'
import type { TFunction } from 'i18next'
import type {
  CustomScanDirEntry,
  ExploreSourceConfigDto,
  GithubOwnerEntry,
  GithubTokenStatusDto,
  OriginRules,
  DevicePipelineReportDto,
  ToolDirOverride,
  ToolStatusDto,
  WebDavConfigDto,
} from './types'
import {
  LanguageSection,
  StorageSection,
  GitCacheSection,
  GithubTokenSection,
  OriginRulesSection,
  ExploreSourcesSection,
  ToolDirSection,
  ScanDirsSection,
  UpdateSection,
  StatusOverviewSection,
  BackupSyncSection,
} from './settings'

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
  onValidateGithubToken: (token: string) => Promise<GithubTokenStatusDto>
  toolStatus: ToolStatusDto | null
  webdav: WebDavConfigDto | null
  onSaveWebdav: (webdav: WebDavConfigDto) => Promise<void>
  onBackupWebdav: () => Promise<void>
  onListGithubOwners: () => Promise<GithubOwnerEntry[]>
  onDevicePull: () => Promise<DevicePipelineReportDto>
  onDevicePublish: () => Promise<DevicePipelineReportDto>
  onDeviceStatus: () => Promise<DevicePipelineReportDto>
  onUpdateSkills: () => Promise<number>
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
  onValidateGithubToken,
  toolStatus,
  webdav,
  onSaveWebdav,
  onBackupWebdav,
  onListGithubOwners,
  onDevicePull,
  onDevicePublish,
  onDeviceStatus,
  onUpdateSkills,
}: SettingsPageProps) => {
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
        <LanguageSection
          language={language}
          onToggleLanguage={onToggleLanguage}
          t={t}
        />

        <StorageSection
          storagePath={storagePath}
          onPickStoragePath={onPickStoragePath}
          t={t}
        />

        <GitCacheSection
          gitCacheCleanupDays={gitCacheCleanupDays}
          gitCacheTtlSecs={gitCacheTtlSecs}
          onGitCacheCleanupDaysChange={onGitCacheCleanupDaysChange}
          onGitCacheTtlSecsChange={onGitCacheTtlSecsChange}
          onClearGitCacheNow={onClearGitCacheNow}
          t={t}
        />

        <GithubTokenSection
          isTauri={isTauri}
          githubToken={githubToken}
          onGithubTokenChange={onGithubTokenChange}
          onValidateGithubToken={onValidateGithubToken}
          t={t}
        />

        <OriginRulesSection
          isTauri={isTauri}
          originRules={originRules}
          onOriginRulesChange={onOriginRulesChange}
          onListGithubOwners={onListGithubOwners}
          t={t}
        />

        <ExploreSourcesSection
          exploreSources={exploreSources}
          onSaveExploreSources={onSaveExploreSources}
          t={t}
        />

        <ToolDirSection
          isTauri={isTauri}
          toolDirOverrides={toolDirOverrides}
          toolStatus={toolStatus}
          onSetToolDirOverride={onSetToolDirOverride}
          onResetToolDirOverride={onResetToolDirOverride}
          t={t}
        />

        <ScanDirsSection
          customScanDirs={customScanDirs}
          onAddCustomScanDir={onAddCustomScanDir}
          onRemoveCustomScanDir={onRemoveCustomScanDir}
          t={t}
        />

        <UpdateSection isTauri={isTauri} t={t} />

        <StatusOverviewSection
          t={t}
        />

        <BackupSyncSection
          isTauri={isTauri}
          webdav={webdav}
          onSaveWebdav={onSaveWebdav}
          onBackupWebdav={onBackupWebdav}
          onDevicePull={onDevicePull}
          onDevicePublish={onDevicePublish}
          onDeviceStatus={onDeviceStatus}
          onUpdateSkills={onUpdateSkills}
          t={t}
        />

      </div>
    </div>
  )
}

export default memo(SettingsPage)
