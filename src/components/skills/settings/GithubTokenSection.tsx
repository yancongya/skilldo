import { memo, useCallback, useEffect, useState } from 'react'
import type { TFunction } from 'i18next'
import type { GithubTokenStatusDto } from '../types'

type GithubTokenSectionProps = {
  isTauri: boolean
  githubToken: string
  onGithubTokenChange: (token: string) => void
  onValidateGithubToken: (token: string) => Promise<GithubTokenStatusDto>
  t: TFunction
}

const GithubTokenSection = memo(function GithubTokenSection({
  isTauri,
  githubToken,
  onGithubTokenChange,
  onValidateGithubToken,
  t,
}: GithubTokenSectionProps) {
  const [localToken, setLocalToken] = useState(githubToken)
  const [tokenStatus, setTokenStatus] = useState<GithubTokenStatusDto | null>(null)
  const [validating, setValidating] = useState(false)

  useEffect(() => {
    setLocalToken(githubToken)
  }, [githubToken])

  const handleValidateToken = useCallback(async () => {
    if (!isTauri) return
    setValidating(true)
    setTokenStatus(null)
    try {
      const status = await onValidateGithubToken(localToken)
      setTokenStatus(status)
    } catch (err) {
      setTokenStatus({
        valid: false,
        scopes: [],
        error: err instanceof Error ? err.message : String(err),
      })
    } finally {
      setValidating(false)
    }
  }, [isTauri, localToken, onValidateGithubToken])

  return (
    <div className="settings-field">
      <label className="settings-label" htmlFor="settings-github-token">
        {t('githubToken')}
      </label>
      <div className="settings-input-row">
        <input
          id="settings-github-token"
          className="settings-input mono"
          type="password"
          placeholder={t('githubTokenPlaceholder')}
          value={localToken}
          onChange={(e) => setLocalToken(e.target.value)}
          onBlur={() => {
            if (localToken !== githubToken) {
              onGithubTokenChange(localToken)
            }
          }}
        />
        <button
          className="btn btn-secondary settings-browse"
          type="button"
          onClick={handleValidateToken}
          disabled={validating || !localToken.trim()}
        >
          {validating ? t('githubTokenValidating') : t('githubTokenValidate')}
        </button>
      </div>
      <div className="settings-helper">{t('githubTokenHint')}</div>
      {tokenStatus && (
        <div
          className={`settings-token-status ${tokenStatus.valid ? 'ok' : 'err'}`}
        >
          {tokenStatus.valid ? t('githubTokenValid') : t('githubTokenInvalid')}
          {tokenStatus.login && ` · ${t('githubTokenLogin')}: ${tokenStatus.login}`}
          {tokenStatus.scopes.length > 0 &&
            ` · ${t('githubTokenScopes')}: ${tokenStatus.scopes.join(', ')}`}
          {tokenStatus.error && ` · ${tokenStatus.error}`}
        </div>
      )}
    </div>
  )
})

export default GithubTokenSection