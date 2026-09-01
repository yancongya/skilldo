import { memo, useEffect, useState } from 'react'
import type { TFunction } from 'i18next'
import type { ManagedSkill } from '../types'

export type PublishParams = {
  repoName?: string
  owner?: string
  privateRepo: boolean
  message?: string
}

type PublishSkillModalProps = {
  open: boolean
  skill: ManagedSkill | null
  defaultOwner?: string
  onClose: () => void
  onPublish: (params: PublishParams) => Promise<void>
  t: TFunction
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 100)
}

const PublishSkillModal = ({
  open,
  skill,
  defaultOwner = '',
  onClose,
  onPublish,
  t,
}: PublishSkillModalProps) => {
  const [repoName, setRepoName] = useState('')
  const [owner, setOwner] = useState('')
  const [privateRepo, setPrivateRepo] = useState(false)
  const [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Reset the form whenever a different skill is targeted.
  useEffect(() => {
    if (!skill) return
    setRepoName(slugify(skill.name))
    setOwner('')
    setPrivateRepo(false)
    setMessage('')
    setError(null)
    setBusy(false)
  }, [skill?.id, skill])

  if (!open || !skill) return null

  const ownerOrYou = owner.trim() || defaultOwner.trim() || 'your-account'
  const repoOrSlug = repoName.trim() || slugify(skill.name)

  const handleConfirm = async () => {
    setBusy(true)
    setError(null)
    try {
      await onPublish({
        repoName: repoName.trim() || undefined,
        owner: owner.trim() || undefined,
        privateRepo,
        message: message.trim() || undefined,
      })
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="modal-backdrop" onClick={busy ? undefined : onClose}>
      <div className="modal publish-skill-modal" onClick={(event) => event.stopPropagation()}>
        <div className="modal-header">
          <div className="modal-title">{t('publishSkillTitle')}</div>
          <button
            className="modal-close"
            type="button"
            onClick={onClose}
            aria-label={t('close')}
            disabled={busy}
          >
            ✕
          </button>
        </div>
        <div className="modal-body">
          <div className="publish-plan">
            <span>{t('publishSkillSubtitle')}</span>
            <code>
              github.com/{ownerOrYou}/{repoOrSlug}
            </code>
            <span className={`repo-visibility ${privateRepo ? 'is-private' : 'is-public'}`}>
              {privateRepo ? t('repoPrivate') : t('repoPublic')}
            </span>
          </div>

          <div className="form-group">
            <label className="label">{t('publishRepoName')}</label>
            <input
              className="input"
              value={repoName}
              onChange={(event) => setRepoName(event.target.value)}
              placeholder={slugify(skill.name)}
              disabled={busy}
            />
          </div>

          <div className="form-group">
            <label className="label">{t('publishOwner')}</label>
            <input
              className="input"
              value={owner}
              onChange={(event) => setOwner(event.target.value)}
              placeholder={defaultOwner.trim() || 'your-account'}
              disabled={busy}
            />
            <div className="helper-text">{t('publishOwnerHint')}</div>
          </div>

          <div className="form-group">
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={privateRepo}
                onChange={(event) => setPrivateRepo(event.target.checked)}
                disabled={busy}
              />
              <span>{t('publishPrivate')}</span>
            </label>
          </div>

          <div className="form-group">
            <label className="label">{t('publishMessage')}</label>
            <textarea
              className="input textarea"
              value={message}
              onChange={(event) => setMessage(event.target.value)}
              placeholder={`Publish ${skill.name}`}
              rows={2}
              disabled={busy}
            />
          </div>

          {error ? <div className="form-error">{error}</div> : null}
        </div>
        <div className="modal-footer">
          <button
            className="btn btn-secondary"
            type="button"
            onClick={onClose}
            disabled={busy}
          >
            {t('cancel')}
          </button>
          <button
            className="btn btn-primary"
            type="button"
            onClick={() => void handleConfirm()}
            disabled={busy}
          >
            {busy ? t('actions.publishing', { name: skill.name }) : t('publishConfirm')}
          </button>
        </div>
      </div>
    </div>
  )
}

export default memo(PublishSkillModal)
