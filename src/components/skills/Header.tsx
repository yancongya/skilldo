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
          <img className="logo-icon" src="/logo.png" alt="" />
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
