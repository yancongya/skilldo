import { memo } from 'react'
import { Copy, RefreshCw, Tag, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import type { TFunction } from 'i18next'
import type { ManagedSkill, UpdateCheckResultDto } from './types'
import SkillIcon from './SkillIcon'
import SkillOriginBadge from './SkillOriginBadge'

type GithubInfo = {
  label: string
  href: string
}

type SkillCardCompactProps = {
  skill: ManagedSkill
  loading: boolean
  updateCheck?: UpdateCheckResultDto
  getGithubInfo: (url: string | null | undefined) => GithubInfo | null
  getSkillSourceLabel: (skill: ManagedSkill) => string
  formatRelative: (ms: number | null | undefined) => string
  onUpdate: (skill: ManagedSkill) => void
  onDelete: (skillId: string) => void
  onOpenScope: (skill: ManagedSkill) => void
  onOpenDetail: (skill: ManagedSkill) => void
  onEditTags: (skill: ManagedSkill) => void
  getSkillScope: (skill: ManagedSkill) => 'global' | 'project'
  getSkillProjects: (skill: ManagedSkill) => string[]
  t: TFunction
}

const SkillCardCompact = ({
  skill,
  loading,
  updateCheck,
  getGithubInfo,
  getSkillSourceLabel,
  formatRelative,
  onUpdate,
  onDelete,
  onOpenScope,
  onOpenDetail,
  onEditTags,
  getSkillScope,
  getSkillProjects,
  t,
}: SkillCardCompactProps) => {
  const github = getGithubInfo(skill.source_ref)
  const copyValue = (github?.href ?? skill.source_ref ?? '').trim()
  const skillScope = getSkillScope(skill)
  const projectCount = getSkillProjects(skill).length
  const sourceLabel = github?.label ?? getSkillSourceLabel(skill)
  const description = skill.description?.trim() || t('noSkillDescription')

  const handleCopy = async () => {
    if (!copyValue) return
    try {
      await navigator.clipboard.writeText(copyValue)
      toast.success(t('copied'))
    } catch {
      toast.error(t('copyFailed'))
    }
  }

  return (
    <div className="skill-card-compact">
      <button
        type="button"
        className="skill-card-compact-clickable"
        onClick={() => onOpenDetail(skill)}
      >
        <div className="skill-card-compact-header">
          <SkillIcon skill={skill} size="md" />
          <div className="skill-card-compact-info">
            <div className="skill-card-compact-name">{skill.name}</div>
            <div className="skill-card-compact-source">{sourceLabel}</div>
          </div>
          <div className="skill-card-compact-badges">
            <SkillOriginBadge skill={skill} t={t} compact />
            {updateCheck?.has_update || updateCheck?.has_local_changes ? (
              <span className="compact-update-chip">
                {updateCheck.has_local_changes && !updateCheck.has_update
                  ? t('pushAvailableShort')
                  : t('updateAvailableShort')}
              </span>
            ) : null}
            <span className={`compact-scope-chip ${skillScope}`}>
              {skillScope === 'project' ? projectCount : t('scope.globalBadge')}
            </span>
          </div>
        </div>
        <div className="skill-card-compact-desc">{description}</div>
        <div className="skill-card-compact-bottom">
          <span>{formatRelative(skill.updated_at)}</span>
          {skill.tags[0] ? <span>#{skill.tags[0].name}</span> : null}
        </div>
      </button>
      <div className="skill-card-compact-actions">
        <button
          className="card-btn"
          type="button"
          title={copyValue || sourceLabel}
          onClick={() => void handleCopy()}
          disabled={!copyValue || loading}
          aria-label={t('copy')}
        >
          <Copy size={13} />
        </button>
        <button
          className={`card-btn scope-action ${skillScope}`}
          type="button"
          onClick={() => onOpenScope(skill)}
          disabled={loading}
          aria-label={t('scope.filterLabel')}
          title={
            skillScope === 'project'
              ? t('scope.projectCount', { count: projectCount })
              : t('scope.globalBadge')
          }
        >
          {skillScope === 'project' ? projectCount : 'G'}
        </button>
        <button
          className="card-btn tag-action"
          type="button"
          onClick={() => onEditTags(skill)}
          disabled={loading}
          aria-label={t('editTags')}
          title={t('editTags')}
        >
          <Tag size={13} />
        </button>
        <button
          className="card-btn primary-action"
          type="button"
          onClick={() => onUpdate(skill)}
          disabled={loading}
          aria-label={t('update')}
          title={updateCheck?.has_update ? t('detail.updateFromSource') : t('update')}
        >
          <RefreshCw size={13} />
        </button>
        <button
          className="card-btn danger-action"
          type="button"
          onClick={() => onDelete(skill.id)}
          disabled={loading}
          aria-label={t('remove')}
        >
          <Trash2 size={13} />
        </button>
      </div>
    </div>
  )
}

export default memo(SkillCardCompact)
