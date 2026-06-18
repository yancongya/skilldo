import { memo, useState, useEffect, useRef } from 'react'
import type { TFunction } from 'i18next'

type NamePromptModalProps = {
  pendingPath: string
  t: TFunction
  onConfirm: (name: string) => void
  onCancel: () => void
}

const NamePromptModal = ({
  pendingPath,
  t,
  onConfirm,
  onCancel,
}: NamePromptModalProps) => {
  const defaultName = pendingPath.split('/').pop() || pendingPath.split('\\').pop() || 'custom'
  const [name, setName] = useState(defaultName)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    setTimeout(() => inputRef.current?.focus(), 0)
  }, [])

  const handleConfirm = () => {
    const trimmed = name.trim()
    if (!trimmed) return
    onConfirm(trimmed)
  }

  return (
    <div className="modal-backdrop" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
        <div className="modal-header">
          <div className="modal-title">{t('addScanDir')}</div>
          <button
            className="modal-close"
            type="button"
            onClick={onCancel}
            aria-label={t('close')}
          >
            ✕
          </button>
        </div>
        <div className="modal-body">
          <p className="label">{t('customScanDirNamePrompt')}</p>
          <input
            ref={inputRef}
            className="settings-input"
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleConfirm()
            }}
          />
          <div className="settings-helper mono" style={{ marginTop: 8 }}>
            {pendingPath}
          </div>
        </div>
        <div className="modal-footer">
          <button className="btn btn-secondary" type="button" onClick={onCancel}>
            {t('cancel')}
          </button>
          <button
            className="btn btn-primary"
            type="button"
            onClick={handleConfirm}
            disabled={!name.trim()}
          >
            {t('done')}
          </button>
        </div>
      </div>
    </div>
  )
}

export default memo(NamePromptModal)
