import { memo, useMemo, useState } from 'react'
import type { TFunction } from 'i18next'
import type { ToolDirOverride, ToolStatusDto } from '../types'
import ToolDirRow from './ToolDirRow'

type ToolDirSectionProps = {
  isTauri: boolean
  toolDirOverrides: ToolDirOverride[]
  toolStatus: ToolStatusDto | null
  onSetToolDirOverride: (toolKey: string, path: string) => void
  onResetToolDirOverride: (toolKey: string) => void
  t: TFunction
}

const ToolDirSection = memo(function ToolDirSection({
  isTauri,
  toolDirOverrides,
  toolStatus,
  onSetToolDirOverride,
  onResetToolDirOverride,
  t,
}: ToolDirSectionProps) {
  const [hideInstalled, setHideInstalled] = useState(false)
  const [hideCustom, setHideCustom] = useState(false)
  const tools = useMemo(() => {
    const statusByKey = new Map((toolStatus?.tools ?? []).map((tool) => [tool.key, tool]))
    return toolDirOverrides.filter((tdo) => {
      const status = statusByKey.get(tdo.tool_key)
      if (hideInstalled && status?.installed) return false
      if (hideCustom && tdo.has_override) return false
      return true
    })
  }, [hideCustom, hideInstalled, toolDirOverrides, toolStatus])

  return (
    <>
      <div className="settings-section-divider" />
      <div className="settings-section-title">{t('toolDirOverrideTitle')}</div>
      <div className="settings-helper" style={{ marginBottom: 12 }}>{t('toolDirOverrideHint')}</div>
      <div className="settings-tool-filters" role="group" aria-label={t('toolFilters')}>
        <label>
          <input type="checkbox" checked={hideInstalled} onChange={(event) => setHideInstalled(event.target.checked)} />
          {t('hideInstalledTools')}
        </label>
        <label>
          <input type="checkbox" checked={hideCustom} onChange={(event) => setHideCustom(event.target.checked)} />
          {t('hideConfiguredTools')}
        </label>
        <span className="settings-helper">{t('toolFilterCount', { visible: tools.length, total: toolDirOverrides.length })}</span>
      </div>
      <div className="settings-tool-grid">
      {tools.map((tdo) => (
        <ToolDirRow
          key={tdo.tool_key}
          tdo={tdo}
          isTauri={isTauri}
          onSet={onSetToolDirOverride}
          onReset={onResetToolDirOverride}
          t={t}
          status={toolStatus?.tools.find((tool) => tool.key === tdo.tool_key) ?? null}
        />
      ))}
      </div>
    </>
  )
})

export default ToolDirSection
