import { memo } from 'react'
import type { TFunction } from 'i18next'

type LanguageSectionProps = {
  language: string
  onToggleLanguage: () => void
  t: TFunction
}

const LanguageSection = memo(function LanguageSection({
  language,
  onToggleLanguage,
  t,
}: LanguageSectionProps) {
  return (
    <div className="settings-field">
      <label className="settings-label" htmlFor="settings-language">
        {t('interfaceLanguage')}
      </label>
      <div className="settings-select-wrap">
        <select
          id="settings-language"
          className="settings-select"
          value={language}
          onChange={(event) => {
            if (event.target.value !== language) {
              onToggleLanguage()
            }
          }}
        >
          <option value="en">{t('languageOptions.en')}</option>
          <option value="zh">{t('languageOptions.zh')}</option>
        </select>
        <svg
          className="settings-select-caret"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          aria-hidden="true"
        >
          <path d="M6 9l6 6 6-6" />
        </svg>
      </div>
    </div>
  )
})

export default LanguageSection