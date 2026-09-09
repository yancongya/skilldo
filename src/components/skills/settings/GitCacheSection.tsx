import { memo, useCallback, useRef } from 'react'
import type { TFunction } from 'i18next'

type GitCacheSectionProps = {
  gitCacheCleanupDays: number
  gitCacheTtlSecs: number
  onGitCacheCleanupDaysChange: (nextDays: number) => void
  onGitCacheTtlSecsChange: (nextSecs: number) => void
  onClearGitCacheNow: () => void
  t: TFunction
}

const GitCacheSection = memo(function GitCacheSection({
  gitCacheCleanupDays,
  gitCacheTtlSecs,
  onGitCacheCleanupDaysChange,
  onGitCacheTtlSecsChange,
  onClearGitCacheNow,
  t,
}: GitCacheSectionProps) {
  const cleanupDaysTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const ttlSecsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const handleCleanupDaysChange = useCallback((nextDays: number) => {
    if (cleanupDaysTimerRef.current) {
      clearTimeout(cleanupDaysTimerRef.current)
    }
    cleanupDaysTimerRef.current = setTimeout(() => {
      // 清空输入框时恢复默认值（天数：30）
      onGitCacheCleanupDaysChange(nextDays === 0 ? 30 : nextDays)
    }, 300)
  }, [onGitCacheCleanupDaysChange])

  const handleTtlSecsChange = useCallback((nextSecs: number) => {
    if (ttlSecsTimerRef.current) {
      clearTimeout(ttlSecsTimerRef.current)
    }
    ttlSecsTimerRef.current = setTimeout(() => {
      // 清空输入框时恢复默认值（TTL：3600）
      onGitCacheTtlSecsChange(nextSecs === 0 ? 3600 : nextSecs)
    }, 300)
  }, [onGitCacheTtlSecsChange])

  return (
    <>
      <div className="settings-field">
        <label className="settings-label" htmlFor="settings-git-cache-days">
          {t('gitCacheCleanupDays')}
        </label>
        <div className="settings-input-row">
          <input
            id="settings-git-cache-days"
            className="settings-input"
            type="number"
            min={0}
            max={3650}
            step={1}
            value={gitCacheCleanupDays}
            onChange={(event) => {
              const next = Number(event.target.value)
              if (!Number.isNaN(next)) {
                handleCleanupDaysChange(next)
              }
            }}
          />
          <button
            className="btn btn-secondary settings-browse"
            type="button"
            onClick={onClearGitCacheNow}
          >
            {t('cleanNow')}
          </button>
        </div>
        <div className="settings-helper">{t('gitCacheCleanupHint')}</div>
      </div>

      <div className="settings-field">
        <label className="settings-label" htmlFor="settings-git-cache-ttl">
          {t('gitCacheTtlSecs')}
        </label>
        <div className="settings-input-row">
          <input
            id="settings-git-cache-ttl"
            className="settings-input"
            type="number"
            min={0}
            max={3600}
            step={1}
            value={gitCacheTtlSecs}
            onChange={(event) => {
              const next = Number(event.target.value)
              if (!Number.isNaN(next)) {
                handleTtlSecsChange(next)
              }
            }}
          />
        </div>
        <div className="settings-helper">{t('gitCacheTtlHint')}</div>
      </div>
    </>
  )
})

export default GitCacheSection