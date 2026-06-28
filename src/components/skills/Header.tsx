import { memo } from 'react'
import { Layers, Moon, RefreshCw, Search, Settings, Sun, Tag } from 'lucide-react'
import type { TFunction } from 'i18next'

type HeaderProps = {
  language: string
  loading: boolean
  resolvedTheme: 'light' | 'dark'
  activeView: 'myskills' | 'explore' | 'detail' | 'settings' | 'tags'
  onToggleLanguage: () => void
  onToggleTheme: () => void
  onOpenSettings: () => void
  onViewChange: (view: 'myskills' | 'explore' | 'tags') => void
  onRefresh: () => void
  t: TFunction
}

const Header = ({
  language,
  resolvedTheme,
  activeView,
  onToggleLanguage,
  onToggleTheme,
  onOpenSettings,
  onViewChange,
  onRefresh,
  t,
}: HeaderProps) => {
  return (
    <header className="skills-header">
      <div className="header-left">
        <div className="brand-area">
          <svg className="logo-icon" viewBox="0 0 40 40" role="img" aria-label={t('appName')}>
            <defs>
              <linearGradient id="skillsHubLogoMark" x1="8" y1="6" x2="32" y2="34" gradientUnits="userSpaceOnUse">
                <stop offset="0" stopColor="#38bdf8" />
                <stop offset="0.52" stopColor="#2563eb" />
                <stop offset="1" stopColor="#7c3aed" />
              </linearGradient>
            </defs>
            <rect x="4" y="4" width="32" height="32" rx="9" fill="url(#skillsHubLogoMark)" />
            <path
              d="M20 10.5 28.2 15v10L20 29.5 11.8 25V15L20 10.5Z"
              fill="none"
              stroke="white"
              strokeWidth="2.2"
              strokeLinejoin="round"
            />
            <path
              d="M12.2 15.2 20 19.6l7.8-4.4M20 19.6v9.2"
              fill="none"
              stroke="white"
              strokeWidth="1.8"
              strokeLinecap="round"
              strokeLinejoin="round"
              opacity="0.92"
            />
            <circle cx="20" cy="10.5" r="2.2" fill="#dbeafe" />
            <circle cx="28.2" cy="25" r="2.2" fill="#dbeafe" />
            <circle cx="11.8" cy="25" r="2.2" fill="#dbeafe" />
          </svg>
          <div className="brand-text-wrap">
            <div className="brand-text">{t('appName')}</div>
          </div>
        </div>
        <nav className="nav-tabs">
          <button
            className={`nav-tab${activeView === 'myskills' || activeView === 'detail' ? ' active' : ''}`}
            type="button"
            onClick={() => onViewChange('myskills')}
          >
            <Layers size={16} />
            {t('navMySkills')}
          </button>
          <button
            className={`nav-tab${activeView === 'explore' ? ' active' : ''}`}
            type="button"
            onClick={() => onViewChange('explore')}
          >
            <Search size={16} />
            {t('navExplore')}
          </button>
          <button
            className={`nav-tab${activeView === 'tags' ? ' active' : ''}`}
            type="button"
            onClick={() => onViewChange('tags')}
          >
            <Tag size={16} />
            {t('navTags')}
          </button>
        </nav>
      </div>
      <div className="header-actions">
        <button className="icon-btn" type="button" onClick={onRefresh} title={t('refresh')}>
          <RefreshCw size={16} />
        </button>
        <button
          className="icon-btn"
          type="button"
          onClick={onToggleTheme}
          title={resolvedTheme === 'dark' ? t('themeToggle.light') : t('themeToggle.dark')}
          aria-label={resolvedTheme === 'dark' ? t('themeToggle.light') : t('themeToggle.dark')}
        >
          {resolvedTheme === 'dark' ? <Sun size={17} /> : <Moon size={17} />}
        </button>
        <button className="lang-btn" type="button" onClick={onToggleLanguage}>
          {language === 'en' ? t('languageShort.en') : t('languageShort.zh')}
        </button>
        <button className={`icon-btn${activeView === 'settings' ? ' active' : ''}`} type="button" onClick={onOpenSettings}>
          <Settings size={18} />
        </button>
      </div>
    </header>
  )
}

export default memo(Header)
