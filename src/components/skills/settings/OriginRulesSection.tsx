import { memo, useCallback, useEffect, useState } from 'react'
import { Check } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { GithubOwnerEntry, OriginRules } from '../types'

type OriginRulesSectionProps = {
  isTauri: boolean
  originRules: OriginRules
  onOriginRulesChange: (rules: OriginRules) => void
  onListGithubOwners: () => Promise<GithubOwnerEntry[]>
  t: TFunction
}

const OriginRulesSection = memo(function OriginRulesSection({
  isTauri,
  originRules,
  onOriginRulesChange,
  onListGithubOwners,
  t,
}: OriginRulesSectionProps) {
  const joinRules = (items: string[]) => items.join('\n')
  const parseRules = (value: string) =>
    value
      .split(/[,\n]/)
      .map((item) => item.trim())
      .filter(Boolean)

  const [officialGitReposText, setOfficialGitReposText] = useState(joinRules(originRules.officialGitRepos))
  const [discoveredOwners, setDiscoveredOwners] = useState<GithubOwnerEntry[]>([])
  const [loadingOwners, setLoadingOwners] = useState(false)

  useEffect(() => {
    setOfficialGitReposText(joinRules(originRules.officialGitRepos))
  }, [originRules])

  const handleSaveOriginRules = useCallback(() => {
    onOriginRulesChange({
      myGitOwners: originRules.myGitOwners,
      myGitRepos: originRules.myGitRepos,
      officialGitRepos: parseRules(officialGitReposText),
    })
  }, [originRules.myGitOwners, originRules.myGitRepos, officialGitReposText, onOriginRulesChange])

  const handleDiscoverOwners = useCallback(async () => {
    if (!isTauri) return
    setLoadingOwners(true)
    try {
      const owners = await onListGithubOwners()
      setDiscoveredOwners(owners)
    } catch {
      setDiscoveredOwners([])
    } finally {
      setLoadingOwners(false)
    }
  }, [isTauri, onListGithubOwners])

  const handleAddOwner = useCallback(
    (login: string) => {
      const next = { ...originRules, myGitOwners: [...new Set([...originRules.myGitOwners, login])] }
      onOriginRulesChange(next)
    },
    [originRules, onOriginRulesChange],
  )

  return (
    <>
      <div className="settings-section-divider" />
      <div className="settings-section-title">{t('originRulesTitle')}</div>
      <div className="settings-helper" style={{ marginBottom: 12 }}>{t('originRulesHint')}</div>
      <div className="settings-field">
        <label className="settings-label" htmlFor="settings-origin-official-repos">
          {t('originRules.officialGitRepos')}
        </label>
        <textarea
          id="settings-origin-official-repos"
          className="settings-input settings-textarea mono"
          value={officialGitReposText}
          onChange={(event) => setOfficialGitReposText(event.target.value)}
          placeholder="openai/skills&#10;anthropics/skills"
        />
        <div className="settings-helper">{t('originRules.officialGitReposHint')}</div>
      </div>
      <button className="btn btn-primary btn-sm" type="button" onClick={handleSaveOriginRules}>
        {t('save')}
      </button>

      {/* Discover GitHub owners from repos */}
      {originRules.myGitOwners.length > 0 ? (
        <>
          <div className="settings-field" style={{ marginTop: 16 }}>
            <label className="settings-label">{t('originRules.discoveredOwners')}</label>
            <div className="settings-helper" style={{ marginBottom: 8 }}>
              {t('originRules.discoveredOwnersHint')}
            </div>
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              onClick={handleDiscoverOwners}
              disabled={loadingOwners}
            >
              {loadingOwners ? t('loading') : t('discover')}
            </button>
            {discoveredOwners.length > 0 ? (
              <div className="github-owners-grid">
                {discoveredOwners.map((owner) => {
                  const isAlreadyMine = originRules.myGitOwners.includes(owner.login)
                  return (
                    <div className="github-owner-card" key={owner.login}>
                      {owner.avatarUrl ? (
                        <img src={owner.avatarUrl} alt={owner.login} className="github-owner-avatar" />
                      ) : null}
                      <div className="github-owner-info">
                        <span className="github-owner-login">{owner.login}</span>
                        <span className="github-owner-repo-count">
                          {t('originRules.repoCount', { count: owner.repoCount })}
                        </span>
                      </div>
                      <button
                        className={`btn btn-sm ${isAlreadyMine ? 'btn-disabled' : 'btn-secondary'}`}
                        type="button"
                        onClick={() => handleAddOwner(owner.login)}
                        disabled={isAlreadyMine}
                      >
                        {isAlreadyMine ? <Check size={16} /> : t('originRules.addAsMyOwner')}
                      </button>
                    </div>
                  )
                })}
              </div>
            ) : null}
          </div>
        </>
      ) : (
        <div className="settings-helper" style={{ marginTop: 8, fontStyle: 'italic' }}>
          {t('originRules.noGithubToken')}
        </div>
      )}
    </>
  )
})

export default OriginRulesSection