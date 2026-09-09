import { memo, useCallback, useState } from 'react'
import type { TFunction } from 'i18next'
import type { CustomScanDirEntry } from '../types'

type ScanDirsSectionProps = {
  customScanDirs: CustomScanDirEntry[]
  onAddCustomScanDir: (path?: string) => void
  onRemoveCustomScanDir: (path: string) => void
  t: TFunction
}

const ScanDirsSection = memo(function ScanDirsSection({
  customScanDirs,
  onAddCustomScanDir,
  onRemoveCustomScanDir,
  t,
}: ScanDirsSectionProps) {
  const [manualDirInput, setManualDirInput] = useState('')

  const handleManualAddScanDir = useCallback(() => {
    const trimmed = manualDirInput.trim()
    if (trimmed) {
      onAddCustomScanDir(trimmed)
      setManualDirInput('')
    }
  }, [manualDirInput, onAddCustomScanDir])

  return (
    <>
      <div className="settings-section-divider" />
      <div className="settings-section-title">{t('customScanDirs')}</div>
      <div className="settings-helper" style={{ marginBottom: 12 }}>{t('customScanDirHint')}</div>
      {customScanDirs.map((entry) => (
        <div className="settings-tool-dir-row" key={entry.path}>
          <div className="settings-tool-dir-top">
            <span className="settings-tool-dir-label">{entry.name}</span>
            <div className="settings-tool-dir-actions">
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                onClick={() => onRemoveCustomScanDir(entry.path)}
              >
                {t('remove')}
              </button>
            </div>
          </div>
          <div className="settings-tool-dir-path mono">{entry.path}</div>
        </div>
      ))}
      <div className="settings-input-row" style={{ marginTop: 8 }}>
        <input
          className="settings-input mono"
          type="text"
          placeholder="~/path/to/skills"
          value={manualDirInput}
          onChange={(e) => setManualDirInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              handleManualAddScanDir()
            }
          }}
        />
        <button
          className="btn btn-secondary btn-sm"
          type="button"
          onClick={handleManualAddScanDir}
          disabled={!manualDirInput.trim()}
        >
          {t('addScanDir')}
        </button>
        <button
          className="btn btn-secondary btn-sm"
          type="button"
          onClick={() => onAddCustomScanDir()}
        >
          {t('browse')}
        </button>
      </div>
    </>
  )
})

export default ScanDirsSection