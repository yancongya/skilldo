import { createElement, memo, useMemo, useState } from 'react'
import type { CSSProperties } from 'react'
import {
  BarChart3,
  Brush,
  Cloud,
  Code2,
  Database,
  File,
  FileSpreadsheet,
  FileText,
  FlaskConical,
  Folder,
  Github,
  Image,
  Megaphone,
  Palette,
  Presentation,
  Rocket,
  Search,
  Shield,
  Sparkles,
  Terminal,
  TestTube2,
  Video,
  Wrench,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { ManagedSkill } from './types'

type SkillIconProps = {
  skill: Pick<
    ManagedSkill,
    'name' | 'description' | 'source_type' | 'source_ref' | 'icon_url' | 'icon_emoji' | 'icon_background'
  >
  size?: 'sm' | 'md' | 'lg'
  className?: string
  decorative?: boolean
}

type IconTone = {
  bg: string
  fg: string
  darkBg: string
  darkFg: string
}

const ICON_URL_PATTERN =
  /^data:image\/(?:png|jpeg|jpg|webp|gif|svg\+xml);base64,[A-Za-z0-9+/]+={0,2}$/i
const EXPLICIT_PROTOCOL_PATTERN = /^[a-z][a-z0-9+.-]*:/i

const TONES: IconTone[] = [
  { bg: '#dbeafe', fg: '#1d4ed8', darkBg: 'rgba(37, 99, 235, 0.22)', darkFg: '#93c5fd' },
  { bg: '#fce7f3', fg: '#be185d', darkBg: 'rgba(190, 24, 93, 0.24)', darkFg: '#f9a8d4' },
  { bg: '#dcfce7', fg: '#15803d', darkBg: 'rgba(21, 128, 61, 0.24)', darkFg: '#86efac' },
  { bg: '#ffedd5', fg: '#c2410c', darkBg: 'rgba(194, 65, 12, 0.24)', darkFg: '#fdba74' },
  { bg: '#ede9fe', fg: '#6d28d9', darkBg: 'rgba(109, 40, 217, 0.26)', darkFg: '#c4b5fd' },
  { bg: '#cffafe', fg: '#0e7490', darkBg: 'rgba(14, 116, 144, 0.25)', darkFg: '#67e8f9' },
  { bg: '#e0e7ff', fg: '#4338ca', darkBg: 'rgba(67, 56, 202, 0.25)', darkFg: '#a5b4fc' },
  { bg: '#fef3c7', fg: '#b45309', darkBg: 'rgba(180, 83, 9, 0.24)', darkFg: '#fcd34d' },
  { bg: '#ccfbf1', fg: '#0f766e', darkBg: 'rgba(15, 118, 110, 0.25)', darkFg: '#5eead4' },
  { bg: '#ffe4e6', fg: '#be123c', darkBg: 'rgba(190, 18, 60, 0.24)', darkFg: '#fda4af' },
]

const ICON_RULES: { pattern: RegExp; icon: LucideIcon }[] = [
  { pattern: /\b(pdf|acrobat)\b/i, icon: FileText },
  { pattern: /\b(word|docx?|document)\b/i, icon: FileText },
  { pattern: /\b(excel|spreadsheet|sheet|csv|table)\b/i, icon: FileSpreadsheet },
  { pattern: /\b(powerpoint|ppt|slides?|presentation)\b/i, icon: Presentation },
  { pattern: /\b(github|git|pr|pull request|commit|changelog)\b/i, icon: Github },
  { pattern: /\b(playwright|browser|webapp|test|testing|e2e|ci)\b/i, icon: TestTube2 },
  { pattern: /\b(deploy|vercel|netlify|cloudflare|docker|release)\b/i, icon: Rocket },
  { pattern: /\b(mcp|protocol|builder|integration|connector)\b/i, icon: Wrench },
  { pattern: /\b(ai|prompt|agent|openai|anthropic|generation|generate)\b/i, icon: Sparkles },
  { pattern: /\b(image|screenshot|capture|vision|photo|video)\b/i, icon: Image },
  { pattern: /\b(transcribe|audio|voice|youtube)\b/i, icon: Video },
  { pattern: /\b(jupyter|notebook|python|flask|lab)\b/i, icon: FlaskConical },
  { pattern: /\b(linear|issue|project|task|management|kanban)\b/i, icon: BarChart3 },
  { pattern: /\b(notion|knowledge|notes?|wiki)\b/i, icon: File },
  { pattern: /\b(sentry|security|review|audit|secret|lock)\b/i, icon: Shield },
  { pattern: /\b(figma|design|frontend|css|theme|brand|palette)\b/i, icon: Palette },
  { pattern: /\b(database|postgres|sql|data|analysis|chart)\b/i, icon: Database },
  { pattern: /\b(terminal|cli|shell|command)\b/i, icon: Terminal },
  { pattern: /\b(code|react|html|web|software|architecture)\b/i, icon: Code2 },
  { pattern: /\b(research|search|discover)\b/i, icon: Search },
  { pattern: /\b(cloud|sync|remote)\b/i, icon: Cloud },
  { pattern: /\b(write|copy|content|article)\b/i, icon: Megaphone },
  { pattern: /\b(brush|style|visual)\b/i, icon: Brush },
]

const SIZE_CLASS = {
  sm: 'skill-icon-box sm',
  md: 'skill-icon-box md',
  lg: 'skill-icon-box lg',
}

function resolveIconUrl(iconUrl?: string | null): string {
  const trimmed = iconUrl?.trim() ?? ''
  if (!trimmed) return ''
  if (ICON_URL_PATTERN.test(trimmed)) return trimmed

  try {
    const parsed = new URL(trimmed)
    return parsed.protocol === 'http:' || parsed.protocol === 'https:'
      ? parsed.toString()
      : ''
  } catch {
    return EXPLICIT_PROTOCOL_PATTERN.test(trimmed) ? '' : trimmed
  }
}

function getTone(input: string): IconTone {
  let hash = 0
  for (let i = 0; i < input.length; i += 1) {
    hash = input.charCodeAt(i) + ((hash << 5) - hash)
  }
  return TONES[Math.abs(hash) % TONES.length]
}

function getForeground(backgroundColor: string): string {
  const hex = backgroundColor.trim().replace(/^#/, '')
  if (!/^[0-9a-f]{6}$/i.test(hex)) return 'var(--text-primary)'

  const r = Number.parseInt(hex.slice(0, 2), 16)
  const g = Number.parseInt(hex.slice(2, 4), 16)
  const b = Number.parseInt(hex.slice(4, 6), 16)
  const luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255
  return luminance > 0.72 ? '#111827' : '#f8fafc'
}

function getInferredIcon(skill: SkillIconProps['skill']): LucideIcon | null {
  const haystack = `${skill.name} ${skill.description ?? ''} ${skill.source_ref ?? ''}`
  for (const rule of ICON_RULES) {
    if (rule.pattern.test(haystack)) return rule.icon
  }

  const sourceType = skill.source_type.toLowerCase()
  if (sourceType.includes('git')) return Github
  if (sourceType.includes('local')) return Folder
  return null
}

function getInitial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || '?'
}

function getIconStyle(tone: IconTone, backgroundColor?: string | null): CSSProperties {
  if (backgroundColor) {
    const foregroundColor = getForeground(backgroundColor)
    return {
      '--skill-icon-bg': backgroundColor,
      '--skill-icon-fg': foregroundColor,
      '--skill-icon-bg-dark': backgroundColor,
      '--skill-icon-fg-dark': foregroundColor,
    } as CSSProperties
  }

  return {
    '--skill-icon-bg': tone.bg,
    '--skill-icon-fg': tone.fg,
    '--skill-icon-bg-dark': tone.darkBg,
    '--skill-icon-fg-dark': tone.darkFg,
  } as CSSProperties
}

const SkillIcon = ({ skill, size = 'md', className = '', decorative = true }: SkillIconProps) => {
  const [imgError, setImgError] = useState(false)
  const safeIconUrl = useMemo(() => resolveIconUrl(skill.icon_url), [skill.icon_url])
  const tone = useMemo(() => getTone(skill.name), [skill.name])
  const inferredIcon = getInferredIcon(skill)
  const iconStyle = getIconStyle(tone, skill.icon_background)
  const ariaHidden = decorative ? 'true' : undefined
  const title = decorative ? undefined : skill.name

  if (safeIconUrl && !imgError) {
    return (
      <span
        className={`${SIZE_CLASS[size]} has-image ${className}`}
        style={iconStyle}
      >
        <img
          src={safeIconUrl}
          alt={decorative ? '' : skill.name}
          aria-hidden={ariaHidden}
          className="skill-icon-img"
          loading="lazy"
          onError={() => setImgError(true)}
        />
      </span>
    )
  }

  if (skill.icon_emoji) {
    return (
      <span
        className={`${SIZE_CLASS[size]} emoji ${className}`}
        style={iconStyle}
        role={decorative ? undefined : 'img'}
        aria-label={title}
        aria-hidden={ariaHidden}
      >
        {skill.icon_emoji}
      </span>
    )
  }

  if (!inferredIcon) {
    return (
      <span
        className={`${SIZE_CLASS[size]} initial ${className}`}
        style={iconStyle}
        aria-hidden={ariaHidden}
        title={title}
      >
      {getInitial(skill.name)}
      </span>
    )
  }

  return (
    <span
      className={`${SIZE_CLASS[size]} ${className}`}
      style={iconStyle}
      aria-hidden={ariaHidden}
      title={title}
    >
      {createElement(inferredIcon)}
    </span>
  )
}

export default memo(SkillIcon)
