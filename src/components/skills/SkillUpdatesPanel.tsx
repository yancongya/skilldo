import { memo } from 'react'
import { DownloadCloud, RefreshCw, UploadCloud, X } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { ManagedSkill, UpdateCheckResultDto } from './types'
import SkillOriginBadge from './SkillOriginBadge'

type SkillUpdatesPanelProps = {
  open: boolean
  skills: ManagedSkill[]
  updateChecks: Record<string, UpdateCheckResultDto>
  loading: boolean
  onRequestClose: () => void
  onUpdateSkill: (skill: ManagedSkill) => void
  onUpdateAll: (skills: ManagedSkill[]) => void
  onPublishSkill: (skill: ManagedSkill) => void
  t: TFunction
}

const SkillUpdatesPanel = ({
  open,
  skills,
  updateChecks,
  loading,
  onRequestClose,
  onUpdateSkill,
  onUpdateAll,
  onPublishSkill,
  t,
}: SkillUpdatesPanelProps) => {
  const updateSkills = skills.filter((skill) => updateChecks[skill.id]?.has_update)
  if (!open) return null

  return (
    <div className="modal-backdrop" onClick={loading ? undefined : onRequestClose}>
      <section
        className="modal modal-lg skill-updates-modal"
        role="dialog"
        aria-modal="true"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="modal-header">
          <div>
            <div className="modal-title">{t('skillUpdatesTitle')}</div>
            <div className="modal-subtitle">
              {t('skillUpdatesSubtitle', { count: updateSkills.length })}
            </div>
          </div>
          <button
            className="modal-close"
            type="button"
            onClick={onRequestClose}
            disabled={loading}
            aria-label={t('close')}
          >
            <X size={16} />
          </button>
        </div>
        <div className="modal-body skill-updates-body">
          {updateSkills.length === 0 ? (
            <div className="empty">{t('skillUpdatesEmpty')}</div>
          ) : (
            <div className="skill-updates-list">
              {updateSkills.map((skill) => {
                const check = updateChecks[skill.id]
                const canPublish = skill.publish_strategy === 'git_push'
                return (
                  <div className="skill-update-row" key={skill.id}>
                    <div className="skill-update-main">
                      <div className="skill-update-name-row">
                        <span className="skill-update-name">{skill.name}</span>
                        <SkillOriginBadge skill={skill} t={t} compact />
                      </div>
                      <div className="skill-update-meta">
                        {t(`updateStrategy.${skill.update_strategy ?? 'unknown'}`)}
                        {check.latest_revision ? (
                          <span className="skill-update-revision">
                            {check.latest_revision.slice(0, 7)}
                          </span>
                        ) : null}
                      </div>
                    </div>
                    <div className="skill-update-actions">
                      {canPublish ? (
                        <button
                          className="btn btn-secondary btn-sm"
                          type="button"
                          onClick={() => onPublishSkill(skill)}
                          disabled={loading}
                          title={t('detail.pushToRemote')}
                        >
                          <UploadCloud size={14} />
                          {t('push')}
                        </button>
                      ) : null}
                      <button
                        className="btn btn-secondary btn-sm"
                        type="button"
                        onClick={() => onUpdateSkill(skill)}
                        disabled={loading}
                      >
                        <RefreshCw size={14} />
                        {t('update')}
                      </button>
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </div>
        <div className="modal-footer space-between">
          <button
            className="btn btn-secondary"
            type="button"
            onClick={onRequestClose}
            disabled={loading}
          >
            {t('close')}
          </button>
          <button
            className="btn btn-primary"
            type="button"
            onClick={() => onUpdateAll(updateSkills)}
            disabled={loading || updateSkills.length === 0}
          >
            <DownloadCloud size={15} />
            {t('updateAllSkills')}
          </button>
        </div>
      </section>
    </div>
  )
}

export default memo(SkillUpdatesPanel)
