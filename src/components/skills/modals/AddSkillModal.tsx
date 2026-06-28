import { memo } from 'react'
import { Check } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { TagWithCountDto, ToolOption, ToolStatusDto } from '../types'

type AddSkillModalProps = {
  open: boolean
  loading: boolean
  canClose: boolean
  addModalTab: 'local' | 'git' | 'package'
  localPath: string
  localName: string
  gitUrl: string
  gitName: string
  packageName: string
  packageCommand: string
  packageSkillName: string
  tags: TagWithCountDto[]
  selectedTagIds: number[]
  syncTargets: Record<string, boolean>
  installedTools: ToolOption[]
  toolStatus: ToolStatusDto | null
  onRequestClose: () => void
  onTabChange: (tab: 'local' | 'git' | 'package') => void
  onLocalPathChange: (value: string) => void
  onPickLocalPath: () => void
  onLocalNameChange: (value: string) => void
  onGitUrlChange: (value: string) => void
  onGitNameChange: (value: string) => void
  onPackageNameChange: (value: string) => void
  onPackageCommandChange: (value: string) => void
  onPackageSkillNameChange: (value: string) => void
  onToggleTag: (tagId: number) => void
  onSyncTargetChange: (toolId: string, checked: boolean) => void
  onSubmit: () => void
  t: TFunction
}

const AddSkillModal = ({
  open,
  loading,
  canClose,
  addModalTab,
  localPath,
  localName,
  gitUrl,
  gitName,
  packageName,
  packageCommand,
  packageSkillName,
  tags,
  selectedTagIds,
  syncTargets,
  installedTools,
  toolStatus,
  onRequestClose,
  onTabChange,
  onLocalPathChange,
  onPickLocalPath,
  onLocalNameChange,
  onGitUrlChange,
  onGitNameChange,
  onPackageNameChange,
  onPackageCommandChange,
  onPackageSkillNameChange,
  onToggleTag,
  onSyncTargetChange,
  onSubmit,
  t,
}: AddSkillModalProps) => {
  if (!open) return null

  return (
    <div
      className="modal-backdrop"
      onClick={() => (canClose ? onRequestClose() : null)}
    >
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <div className="modal-title">{t('addSkillTitle')}</div>
          <button
            className="modal-close"
            type="button"
            onClick={onRequestClose}
            aria-label={t('close')}
            disabled={!canClose}
          >
            ✕
          </button>
        </div>
        <div className="modal-body">
          <div className="tabs">
            <button
              className={`tab-item${addModalTab === 'local' ? ' active' : ''}`}
              type="button"
              onClick={() => onTabChange('local')}
            >
              {t('localTab')}
            </button>
            <button
              className={`tab-item${addModalTab === 'git' ? ' active' : ''}`}
              type="button"
              onClick={() => onTabChange('git')}
            >
              {t('gitTab')}
            </button>
            <button
              className={`tab-item${addModalTab === 'package' ? ' active' : ''}`}
              type="button"
              onClick={() => onTabChange('package')}
            >
              {t('packageTab')}
            </button>
          </div>

          {addModalTab === 'local' ? (
            <>
              <div className="form-group">
                <label className="label">{t('localFolder')}</label>
                <div className="input-row">
                  <input
                    className="input"
                    placeholder={t('localPathPlaceholder')}
                    value={localPath}
                    onChange={(event) => onLocalPathChange(event.target.value)}
                  />
                  <button
                    className="btn btn-secondary input-action"
                    type="button"
                    onClick={onPickLocalPath}
                    disabled={!canClose}
                  >
                    {t('browse')}
                  </button>
                </div>
              </div>
              <div className="form-group">
                <label className="label">{t('optionalNamePlaceholder')}</label>
                <input
                  className="input"
                  placeholder={t('optionalNamePlaceholder')}
                  value={localName}
                  onChange={(event) => onLocalNameChange(event.target.value)}
                />
              </div>
            </>
          ) : addModalTab === 'git' ? (
            <>
              <div className="form-group">
                <label className="label">{t('repositoryUrl')}</label>
                <input
                  className="input"
                  placeholder={t('gitUrlPlaceholder')}
                  value={gitUrl}
                  onChange={(event) => onGitUrlChange(event.target.value)}
                />
              </div>
              <div className="form-group">
                <label className="label">{t('optionalNamePlaceholder')}</label>
                <input
                  className="input"
                  placeholder={t('optionalNamePlaceholder')}
                  value={gitName}
                  onChange={(event) => onGitNameChange(event.target.value)}
                />
              </div>
            </>
          ) : (
            <>
              <div className="form-group">
                <label className="label">{t('packageName')}</label>
                <input
                  className="input"
                  placeholder={t('packageNamePlaceholder')}
                  value={packageName}
                  onChange={(event) => onPackageNameChange(event.target.value)}
                />
              </div>
              <div className="form-group">
                <label className="label">{t('packageCommand')}</label>
                <input
                  className="input"
                  placeholder={t('packageCommandPlaceholder')}
                  value={packageCommand}
                  onChange={(event) => onPackageCommandChange(event.target.value)}
                />
                <div className="helper-text">{t('packageCommandHint')}</div>
              </div>
              <div className="form-group">
                <label className="label">{t('optionalNamePlaceholder')}</label>
                <input
                  className="input"
                  placeholder={t('optionalNamePlaceholder')}
                  value={packageSkillName}
                  onChange={(event) => onPackageSkillNameChange(event.target.value)}
                />
              </div>
            </>
          )}

          <div className="form-group">
            <label className="label">{t('addTags')}</label>
            {tags.length > 0 ? (
              <div className="add-tags-list">
                {tags.map((tag) => {
                  const selected = selectedTagIds.includes(tag.id)
                  return (
                    <button
                      key={tag.id}
                      className={`add-tag-pill${selected ? ' selected' : ''}`}
                      type="button"
                      onClick={() => onToggleTag(tag.id)}
                    >
                      <span className="add-tag-check">
                        {selected ? <Check size={12} /> : null}
                      </span>
                      <span>#{tag.name}</span>
                    </button>
                  )
                })}
              </div>
            ) : (
              <div className="helper-text">{t('noTagsYet')}</div>
            )}
          </div>

          <div className="form-group">
            <label className="label">{t('installToTools')}</label>
            {toolStatus ? (
              <div className="tool-matrix">
                {installedTools.map((tool) => (
                  <label
                    key={tool.id}
                    className={`tool-pill-toggle${
                      syncTargets[tool.id] ? ' active' : ''
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={Boolean(syncTargets[tool.id])}
                      onChange={(event) =>
                        onSyncTargetChange(tool.id, event.target.checked)
                      }
                    />
                    {syncTargets[tool.id] ? <span className="status-badge" /> : null}
                    {tool.label}
                  </label>
                ))}
              </div>
            ) : (
              <div className="helper-text">{t('detectingTools')}</div>
            )}
            <div className="helper-text">{t('syncAfterCreate')}</div>
          </div>
        </div>
        <div className="modal-footer">
          <button
            className="btn btn-secondary"
            onClick={onRequestClose}
            disabled={!canClose}
          >
            {t('cancel')}
          </button>
          <button
            className="btn btn-primary"
            onClick={onSubmit}
            disabled={loading}
          >
            {addModalTab === 'local' ? t('create') : t('install')}
          </button>
        </div>
      </div>
    </div>
  )
}

export default memo(AddSkillModal)
