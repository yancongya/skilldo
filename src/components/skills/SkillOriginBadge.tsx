import type { TFunction } from 'i18next'
import type { ManagedSkill } from './types'

type SkillOriginKind = 'official' | 'git' | 'package' | 'owned-git' | 'local'

type SkillOrigin = {
  kind: SkillOriginKind
  label: string
  title: string
}

const OFFICIAL_GIT_REPOS = [
  'github.com/anthropics/skills',
  'github.com/openai/codex',
  'github.com/openai/openai-cookbook',
]

const normalizeSource = (source: string | null | undefined) =>
  (source ?? '')
    .trim()
    .toLowerCase()
    .replace(/^git\+/, '')
    .replace(/\.git($|[?#])/, '$1')
    .replace(/^https?:\/\/(www\.)?/, '')

const getSkillOrigin = (skill: ManagedSkill, t: TFunction): SkillOrigin => {
  const backendOrigin = skill.source_origin?.toLowerCase()
  const sourceType = skill.source_type.toLowerCase()
  const source = normalizeSource(skill.source_ref)
  const sourceTitle = skill.source_ref?.trim() || skill.central_path

  if (backendOrigin === 'official') {
    return {
      kind: 'official',
      label: t('origin.official'),
      title: t('origin.officialTitle', { source: sourceTitle }),
    }
  }

  if (backendOrigin === 'git' || backendOrigin === 'third_party_git') {
    return {
      kind: 'git',
      label: t('origin.gitRepo'),
      title: t('origin.gitRepoTitle', {
        source: sourceTitle,
        reason: skill.source_origin_reason ?? '',
      }),
    }
  }

  if (backendOrigin === 'my_git') {
    return {
      kind: 'owned-git',
      label: t('origin.ownedGit'),
      title: t('origin.ownedGitTitle', {
        source: sourceTitle,
        reason: skill.source_origin_reason ?? '',
      }),
    }
  }

  if (backendOrigin === 'package') {
    return {
      kind: 'package',
      label: t('origin.packageSource'),
      title: t('origin.packageSourceTitle', {
        source: sourceTitle,
        reason: skill.source_origin_reason ?? '',
      }),
    }
  }

  if (backendOrigin === 'local' || sourceType.includes('local')) {
    return {
      kind: 'local',
      label: t('origin.local'),
      title: t('origin.localTitle', { source: sourceTitle }),
    }
  }

  if (sourceType.includes('git')) {
    const official = OFFICIAL_GIT_REPOS.some((repo) => source.includes(repo))
    return {
      kind: official ? 'official' : 'git',
      label: official ? t('origin.official') : t('origin.gitRepo'),
      title: official
        ? t('origin.officialTitle', { source: sourceTitle })
        : t('origin.gitRepoTitle', { source: sourceTitle, reason: '' }),
    }
  }

  return {
    kind: 'local',
    label: t('origin.local'),
    title: t('origin.localTitle', { source: sourceTitle }),
  }
}

type SkillOriginBadgeProps = {
  skill: ManagedSkill
  t: TFunction
  compact?: boolean
}

const SkillOriginBadge = ({ skill, t, compact = false }: SkillOriginBadgeProps) => {
  const origin = getSkillOrigin(skill, t)
  return (
    <span
      className={`skill-origin-badge ${origin.kind}${compact ? ' compact' : ''}`}
      title={origin.title}
    >
      {origin.label}
    </span>
  )
}

export default SkillOriginBadge
