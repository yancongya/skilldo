import { memo, useState } from 'react'
import { Copy, RefreshCw, Tag, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import type { TFunction } from 'i18next'
import type { ManagedSkill, ToolOption, UpdateCheckResultDto } from './types'
import SkillIcon from './SkillIcon'
import SkillOriginBadge from './SkillOriginBadge'

type GithubInfo = {
  label: string
  href: string
}

type SkillCardProps = {
  skill: ManagedSkill
  installedTools: ToolOption[]
  loading: boolean
  updateCheck?: UpdateCheckResultDto
  getGithubInfo: (url: string | null | undefined) => GithubInfo | null
  getSkillSourceLabel: (skill: ManagedSkill) => string
  formatRelative: (ms: number | null | undefined) => string
  onUpdate: (skill: ManagedSkill) => void
  onDelete: (skillId: string) => void
  onToggleTool: (skill: ManagedSkill, toolId: string) => void
  onToggleAllTools: (skill: ManagedSkill, enabled: boolean) => void
  onOpenScope: (skill: ManagedSkill) => void
  onOpenDetail: (skill: ManagedSkill) => void
  onEditTags: (skill: ManagedSkill) => void
  getSkillScope: (skill: ManagedSkill) => 'global' | 'project'
  getSkillProjects: (skill: ManagedSkill) => string[]
  t: TFunction
}

const MAX_VISIBLE_BADGES = 5

const SkillCard = ({
  skill,
  installedTools,
  loading,
  updateCheck,
  getGithubInfo,
  getSkillSourceLabel,
  formatRelative,
  onUpdate,
  onDelete,
  onToggleTool,
  onToggleAllTools,
  onOpenScope,
  onOpenDetail,
  onEditTags,
  getSkillScope,
  getSkillProjects,
  t,
}: SkillCardProps) => {
  const github = getGithubInfo(skill.source_ref)
  const copyValue = (github?.href ?? skill.source_ref ?? '').trim()
  const skillScope = getSkillScope(skill)
  const projectCount = getSkillProjects(skill).length

  const handleCopy = async () => {
    if (!copyValue) return
    try {
      await navigator.clipboard.writeText(copyValue)
      toast.success(t('copied'))
    } catch {
      toast.error(t('copyFailed'))
    }
  }

  // Split tools into synced and remaining for badge display
  const syncedTools: { tool: ToolOption; target: (typeof skill.targets)[0] }[] = []
  const unsyncedTools: ToolOption[] = []
  for (const tool of installedTools) {
    const target = skill.targets.find(
      (tgt) => tgt.tool === tool.id && (tgt.scope ?? 'global') === skillScope,
    )
    if (target) {
      syncedTools.push({ tool, target })
    } else {
      unsyncedTools.push(tool)
    }
  }

  const [expanded, setExpanded] = useState(false)
  const visibleSynced = syncedTools.slice(0, MAX_VISIBLE_BADGES)
  const hiddenToolCount = installedTools.length - visibleSynced.length
  const displaySyncedTools = expanded ? syncedTools : visibleSynced
  const displayUnsyncedTools = expanded ? unsyncedTools : []
  const eligibleBulkTools =
    skillScope === 'project' && projectCount === 0
      ? []
      : installedTools.filter(
          (tool) => skillScope !== 'project' || tool.supports_project_scope !== false,
        )
  const allEligibleSynced =
    eligibleBulkTools.length > 0 &&
    eligibleBulkTools.every((tool) =>
      skill.targets.some(
        (target) => target.tool === tool.id && (target.scope ?? 'global') === skillScope,
      ),
    )

  return (
    <div className="skill-card">
      <SkillIcon skill={skill} size="lg" />
      <div className="skill-main">
        <div className="skill-header-row">
          <button
            type="button"
            className="skill-name clickable"
            onClick={() => onOpenDetail(skill)}
          >
            {skill.name}
          </button>
          {skill.tags.length > 0 ? (
            <div className="skill-tags-inline">
              {skill.tags.slice(0, 3).map((tag) => (
                <button
                  key={tag.id}
                  className="skill-tag-pill"
                  type="button"
                  onClick={() => onEditTags(skill)}
                >
                  #{tag.name}
                </button>
              ))}
              {skill.tags.length > 3 ? (
                <button
                  className="skill-tag-pill muted"
                  type="button"
                  onClick={() => onEditTags(skill)}
                >
                  +{skill.tags.length - 3}
                </button>
              ) : null}
            </div>
          ) : null}
          <SkillOriginBadge skill={skill} t={t} />
          {updateCheck?.has_update || updateCheck?.has_local_changes ? (
            <span className="skill-update-badge">
              {updateCheck.has_local_changes && !updateCheck.has_update
                ? t('pushAvailableShort')
                : t('updateAvailableShort')}
            </span>
          ) : null}
        </div>
        {skill.description ? (
          <div className="skill-desc">{skill.description}</div>
        ) : null}
        <div className="skill-meta-row">
          {github ? (
            <div className="skill-source">
              <button
                className="repo-pill copyable"
                type="button"
                title={t('copy')}
                aria-label={t('copy')}
                onClick={() => void handleCopy()}
                disabled={!copyValue}
              >
                {github.label}
                <span className="copy-icon" aria-hidden="true">
                  <Copy size={12} />
                </span>
              </button>
            </div>
          ) : (
            <div className="skill-source">
              <button
                className="repo-pill copyable"
                type="button"
                title={t('copy')}
                aria-label={t('copy')}
                onClick={() => void handleCopy()}
                disabled={!copyValue}
              >
                <span className="mono">{getSkillSourceLabel(skill)}</span>
                <span className="copy-icon" aria-hidden="true">
                  <Copy size={12} />
                </span>
              </button>
            </div>
          )}
          <div className="skill-source time">
            <span className="dot">•</span>
            {formatRelative(skill.updated_at)}
          </div>
          <button
            className={`scope-badge ${skillScope}`}
            type="button"
            onClick={() => onOpenScope(skill)}
          >
            {skillScope === 'project'
              ? t('scope.projectCount', { count: projectCount })
              : t('scope.globalBadge')}
          </button>
        </div>
        <div
          className={`tool-matrix-wrap${expanded ? ' expanded' : ''}`}
          title={t('bulkToggleToolsHint')}
          onDoubleClick={(event) => {
            if (loading) return
            if ((event.target as HTMLElement).closest('.tool-pill')) return
            if (eligibleBulkTools.length === 0) return
            onToggleAllTools(skill, !allEligibleSynced)
          }}
        >
          <div className="tool-matrix">
            {displaySyncedTools.map(({ tool, target }) => (
              <button
                key={`${skill.id}-${tool.id}`}
                type="button"
                className="tool-pill active"
                title={`${tool.label} (${target.mode ?? t('unknown')})`}
                onClick={() => void onToggleTool(skill, tool.id)}
              >
                <span className="status-badge" />
                {tool.label}
              </button>
            ))}
            {displayUnsyncedTools.map((tool) => (
              <button
                key={`${skill.id}-${tool.id}`}
                type="button"
                className="tool-pill inactive"
                title={tool.label}
                onClick={() => void onToggleTool(skill, tool.id)}
              >
                {tool.label}
              </button>
            ))}
            {hiddenToolCount > 0 ? (
              <button
                type="button"
                className="tool-pill more-badge"
                onClick={() => setExpanded((current) => !current)}
              >
                {expanded ? t('collapseTools') : t('moreTools', { count: hiddenToolCount })}
              </button>
            ) : null}
          </div>
        </div>
      </div>
      <div className="skill-actions-col">
        <button
          className={`card-btn tag-action${skill.tags.length > 0 ? ' has-tags' : ''}`}
          type="button"
          onClick={() => onEditTags(skill)}
          disabled={loading}
          aria-label={t('editTags')}
          title={t('editTags')}
        >
          <Tag size={16} />
        </button>
        <button
          className="card-btn primary-action"
          type="button"
          onClick={() => onUpdate(skill)}
          disabled={loading}
          aria-label={t('update')}
          title={updateCheck?.has_update ? t('detail.updateFromSource') : t('update')}
        >
          <RefreshCw size={16} />
        </button>
        <button
          className="card-btn danger-action"
          type="button"
          onClick={() => onDelete(skill.id)}
          disabled={loading}
          aria-label={t('remove')}
        >
          <Trash2 size={16} />
        </button>
      </div>
    </div>
  )
}

export default memo(SkillCard)
