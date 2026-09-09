import { memo } from 'react'
import type { TFunction } from 'i18next'

type StorageSectionProps = {
  storagePath: string
  onPickStoragePath: () => void
  t: TFunction
}

const StorageSection = memo(function StorageSection({
  storagePath,
  onPickStoragePath,
  t,
}: StorageSectionProps) {
  return (
    <div className="settings-field">
      <label className="settings-label" htmlFor="settings-storage">
        {t('skillsStoragePath')}
      </label>
      <div className="settings-input-row">
        <input
          id="settings-storage"
          className="settings-input mono"
          value={storagePath}
          readOnly
        />
        <button
          className="btn btn-secondary settings-browse"
          type="button"
          onClick={onPickStoragePath}
        >
          {t('browse')}
        </button>
      </div>
      <div className="settings-helper">{t('skillsStorageHint')}</div>
    </div>
  )
})

export default StorageSection