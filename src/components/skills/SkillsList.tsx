import { memo } from 'react'
import { MessageCircle } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { ManagedSkill, OnboardingPlan, ToolOption } from './types'
import SkillCard from './SkillCard'
import SkillCardCompact from './SkillCardCompact'

type GithubInfo = {
  label: string
  href: string
}

type SkillsListProps = {
  plan: OnboardingPlan | null
  visibleSkills: ManagedSkill[]
  installedTools: ToolOption[]
  loading: boolean
  viewMode: 'list' | 'grid'
  getGithubInfo: (url: string | null | undefined) => GithubInfo | null
  getSkillSourceLabel: (skill: ManagedSkill) => string
  formatRelative: (ms: number | null | undefined) => string
  onReviewImport: () => void
  onUpdateSkill: (skill: ManagedSkill) => void
  onDeleteSkill: (skillId: string) => void
  onToggleTool: (skill: ManagedSkill, toolId: string) => void
  onToggleAllTools: (skill: ManagedSkill, enabled: boolean) => void
  onOpenScope: (skill: ManagedSkill) => void
  onOpenDetail: (skill: ManagedSkill) => void
  onEditTags: (skill: ManagedSkill) => void
  getSkillScope: (skill: ManagedSkill) => 'global' | 'project'
  getSkillProjects: (skill: ManagedSkill) => string[]
  t: TFunction
}

const SkillsList = ({
  plan,
  visibleSkills,
  installedTools,
  loading,
  viewMode,
  getGithubInfo,
  getSkillSourceLabel,
  formatRelative,
  onReviewImport,
  onUpdateSkill,
  onDeleteSkill,
  onToggleTool,
  onToggleAllTools,
  onOpenScope,
  onOpenDetail,
  onEditTags,
  getSkillScope,
  getSkillProjects,
  t,
}: SkillsListProps) => {
  const isGrid = viewMode === 'grid'
  return (
    <div className={`skills-list${isGrid ? ' skills-grid' : ''}`}>
      {plan && plan.total_skills_found > 0 ? (
        <div className="discovered-banner">
          <div className="banner-left">
            <div className="banner-icon">
              <MessageCircle size={18} />
            </div>
            <div className="banner-content">
              <div className="banner-title">{t('discoveredTitle')}</div>
              <div className="banner-subtitle">
                {t('discoveredCount', { count: plan.total_skills_found })}
              </div>
            </div>
          </div>
          <button
            className="btn btn-warning"
            type="button"
            onClick={onReviewImport}
            disabled={loading}
          >
            {t('reviewImport')}
          </button>
        </div>
      ) : null}

      {visibleSkills.length === 0 ? (
        <div className="empty">{t('skillsEmpty')}</div>
      ) : (
        <>
          {visibleSkills.map((skill) =>
            isGrid ? (
              <SkillCardCompact
                key={skill.id}
                skill={skill}
                loading={loading}
                getGithubInfo={getGithubInfo}
                getSkillSourceLabel={getSkillSourceLabel}
                formatRelative={formatRelative}
                onUpdate={onUpdateSkill}
                onDelete={onDeleteSkill}
                onOpenScope={onOpenScope}
                onOpenDetail={onOpenDetail}
                onEditTags={onEditTags}
                getSkillScope={getSkillScope}
                getSkillProjects={getSkillProjects}
                t={t}
              />
            ) : (
              <SkillCard
                key={skill.id}
                skill={skill}
                installedTools={installedTools}
                loading={loading}
                getGithubInfo={getGithubInfo}
                getSkillSourceLabel={getSkillSourceLabel}
                formatRelative={formatRelative}
                onUpdate={onUpdateSkill}
                onDelete={onDeleteSkill}
                onToggleTool={onToggleTool}
                onToggleAllTools={onToggleAllTools}
                onOpenScope={onOpenScope}
                onOpenDetail={onOpenDetail}
                onEditTags={onEditTags}
                getSkillScope={getSkillScope}
                getSkillProjects={getSkillProjects}
                t={t}
              />
            ),
          )}
        </>
      )}
    </div>
  )
}

export default memo(SkillsList)
