import { memo, useCallback, useState } from 'react'
import { FolderOpen, RotateCcw } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { ToolDirOverride, ToolInfoDto } from '../types'

type ToolDirRowProps = {
  tdo: ToolDirOverride
  isTauri: boolean
  onSet: (toolKey: string, path: string) => void
  onReset: (toolKey: string) => void
  t: TFunction
  status: ToolInfoDto | null
}

const ToolDirRow = memo(function ToolDirRow({
  tdo,
  isTauri,
  onSet,
  onReset,
  t,
  status,
}: ToolDirRowProps) {
  const [editing, setEditing] = useState(false)
  const [inputVal, setInputVal] = useState(tdo.current_dir)

  const handleRevealInFinder = useCallback(async () => {
    if (!isTauri) return
    try {
      const { revealItemInDir } = await import('@tauri-apps/plugin-opener')
      await revealItemInDir(tdo.current_dir)
    } catch {
      // ignore
    }
  }, [isTauri, tdo.current_dir])

  const handleSave = useCallback(() => {
    if (inputVal.trim()) {
      onSet(tdo.tool_key, inputVal.trim())
    }
    setEditing(false)
  }, [inputVal, tdo.tool_key, onSet])

  const handleCancel = useCallback(() => {
    setInputVal(tdo.current_dir)
    setEditing(false)
  }, [tdo.current_dir])

  return (
    <div className="settings-tool-dir-row">
      {editing ? (
        <div className="settings-tool-dir-edit-row">
          <input
            className="settings-input mono"
            value={inputVal}
            onChange={(e) => setInputVal(e.target.value)}
            placeholder={tdo.default_dir}
            autoFocus
          />
          <button className="btn btn-primary btn-sm" type="button" onClick={handleSave}>
            {t('save')}
          </button>
          <button className="btn btn-secondary btn-sm" type="button" onClick={handleCancel}>
            {t('cancel')}
          </button>
        </div>
      ) : (
        <>
          <div className="settings-tool-dir-top">
            <div className="settings-tool-dir-icon" aria-hidden="true"><FolderOpen size={18} /></div>
            <div className="settings-tool-dir-heading">
              <span className="settings-tool-dir-label">{tdo.label || tdo.tool_key}</span>
              <div className="settings-tool-dir-badges">
                {status && <span className={`settings-tool-status-badge ${status.installed ? 'ok' : 'muted'}`}>{status.installed ? t('statusInstalled') : t('statusNotInstalled')}</span>}
                {tdo.has_override && <span className="settings-tool-dir-badge">{t('customDir')}</span>}
              </div>
            </div>
          </div>
          <div className="settings-tool-dir-path mono">{tdo.current_dir}</div>
          <div className="settings-tool-dir-actions">
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                onClick={handleRevealInFinder}
                title={t('openInFinder')}
              >
                <FolderOpen size={14} />
                <span>{t('preview')}</span>
              </button>
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                onClick={() => setEditing(true)}
              >
                {t('edit')}
              </button>
              {tdo.has_override && (
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  onClick={() => onReset(tdo.tool_key)}
                >
                  <RotateCcw size={14} />
                </button>
              )}
          </div>
        </>
      )}
    </div>
  )
})

export default ToolDirRow
