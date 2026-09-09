import { memo, useCallback, useEffect, useState } from 'react'
import { Check } from 'lucide-react'
import type { TFunction } from 'i18next'
import { LOCAL_API_BASE } from '../api'

type AuthorEntry = { owner: string; skill_count: number }
type CurrentAuthor = { name: string; email: string; githubLogin: string; githubUrl: string; source: string }

type StatusOverviewSectionProps = {
  t: TFunction
}

const StatusOverviewSection = memo(function StatusOverviewSection({
  t,
}: StatusOverviewSectionProps) {
  // ---- Authors (from skill_origins) ----
  const [authors, setAuthors] = useState<AuthorEntry[]>([])
  const [currentAuthor, setCurrentAuthor] = useState<CurrentAuthor>({ name: '', email: '', githubLogin: '', githubUrl: '', source: '' })
  const [selectedOwner, setSelectedOwner] = useState<string | null>(null)

  useEffect(() => {
    // Load authors list
    fetch(`${LOCAL_API_BASE}/api/authors`)
      .then((r) => r.ok ? r.json() : [])
      .then((data) => setAuthors(data as AuthorEntry[]))
      .catch(() => {})
    // Load current author
    fetch(`${LOCAL_API_BASE}/api/current-author`)
      .then((r) => r.ok ? r.json() : null)
      .then((data) => {
        if (data) {
          setCurrentAuthor(data as CurrentAuthor)
          if (data.githubLogin) setSelectedOwner(data.githubLogin)
        }
      })
      .catch(() => {})
  }, [])

  const handleSelectAuthor = useCallback(async (owner: string) => {
    setSelectedOwner(owner)
    const next: CurrentAuthor = { ...currentAuthor, githubLogin: owner, name: currentAuthor.name || owner }
    setCurrentAuthor(next)
    try {
      await fetch(`${LOCAL_API_BASE}/api/current-author`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(next),
      })
    } catch { /* ignore */ }
  }, [currentAuthor])

  const handleAuthorFieldChange = useCallback(async (field: keyof CurrentAuthor, value: string) => {
    const next = { ...currentAuthor, [field]: value }
    setCurrentAuthor(next)
    try {
      await fetch(`${LOCAL_API_BASE}/api/current-author`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(next),
      })
    } catch { /* ignore */ }
  }, [currentAuthor])

  return (
    <>
      <div className="settings-section-divider" />
      <div className="settings-section-divider" />
      <div className="settings-section-title">{t('currentAuthorTitle')}</div>
      <div className="settings-helper" style={{ marginBottom: 12 }}>
        {t('currentAuthorHint')}
      </div>

      {authors.length > 0 && (
        <div className="settings-author-list">
          {authors.map((a) => (
            <div
              className={`settings-author-item ${selectedOwner === a.owner ? 'selected' : ''}`}
              key={a.owner}
              onClick={() => void handleSelectAuthor(a.owner)}
            >
              <span className="settings-author-name">{a.owner}</span>
              <span className="settings-author-count">{t('skillsCount', { count: a.skill_count })}</span>
              {selectedOwner === a.owner && <Check size={16} className="settings-author-check" />}
            </div>
          ))}
        </div>
      )}
      {authors.length === 0 && (
        <div className="settings-helper" style={{ fontStyle: 'italic' }}>
          {t('currentAuthorNoAuthors')}
        </div>
      )}

      <div className="settings-webdav-grid" style={{ marginTop: 12 }}>
        <label className="settings-field">
          <span>{t('currentAuthorName')}</span>
          <input
            type="text"
            value={currentAuthor.name}
            onChange={(e) => void handleAuthorFieldChange('name', e.target.value)}
            placeholder={t('currentAuthorNamePlaceholder')}
          />
        </label>
        <label className="settings-field">
          <span>{t('currentAuthorEmail')}</span>
          <input
            type="text"
            value={currentAuthor.email}
            onChange={(e) => void handleAuthorFieldChange('email', e.target.value)}
            placeholder={t('currentAuthorEmailPlaceholder')}
          />
        </label>
        <label className="settings-field">
          <span>{t('currentAuthorGithubLogin')}</span>
          <input
            type="text"
            value={currentAuthor.githubLogin}
            onChange={(e) => void handleAuthorFieldChange('githubLogin', e.target.value)}
            placeholder={t('currentAuthorGithubLoginPlaceholder')}
          />
        </label>
        <label className="settings-field">
          <span>{t('currentAuthorGithubUrl')}</span>
          <input
            type="text"
            value={currentAuthor.githubUrl}
            onChange={(e) => void handleAuthorFieldChange('githubUrl', e.target.value)}
            placeholder={t('currentAuthorGithubUrlPlaceholder')}
          />
        </label>
      </div>
    </>
  )
})

export default StatusOverviewSection
