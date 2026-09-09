import { memo, useCallback, useState } from 'react'
import type { TFunction } from 'i18next'
import type { ExploreSourceConfigDto } from '../types'

type ExploreSourcesSectionProps = {
  exploreSources: ExploreSourceConfigDto[]
  onSaveExploreSources: (sources: ExploreSourceConfigDto[]) => void
  t: TFunction
}

const ExploreSourcesSection = memo(function ExploreSourcesSection({
  exploreSources,
  onSaveExploreSources,
  t,
}: ExploreSourcesSectionProps) {
  const [sourceEditor, setSourceEditor] = useState<{
    source: ExploreSourceConfigDto
    isNew: boolean
  } | null>(null)

  const saveSources = useCallback(
    (next: ExploreSourceConfigDto[]) => {
      onSaveExploreSources(next)
    },
    [onSaveExploreSources],
  )

  const startAddSource = useCallback(() => {
    setSourceEditor({
      source: { id: '', name: '', kind: 'featured_json', endpoint: '', enabled: true, builtin: false },
      isNew: true,
    })
  }, [])

  const startEditSource = useCallback((s: ExploreSourceConfigDto) => {
    setSourceEditor({ source: { ...s }, isNew: false })
  }, [])

  const commitSource = useCallback(() => {
    if (!sourceEditor) return
    const draft = sourceEditor.source
    if (!draft.name.trim()) return
    const id =
      draft.id.trim() || `custom-${draft.name.trim().toLowerCase().replace(/\s+/g, '-')}`
    const next = sourceEditor.isNew
      ? [...exploreSources, { ...draft, id }]
      : exploreSources.map((x) => (x.id === id ? { ...draft, id } : x))
    saveSources(next)
    setSourceEditor(null)
  }, [sourceEditor, exploreSources, saveSources])

  const deleteSource = useCallback(
    (id: string) => {
      saveSources(exploreSources.filter((x) => x.id !== id))
    },
    [exploreSources, saveSources],
  )

  const toggleSource = useCallback(
    (id: string) => {
      saveSources(
        exploreSources.map((x) =>
          x.id === id ? { ...x, enabled: !x.enabled } : x,
        ),
      )
    },
    [exploreSources, saveSources],
  )

  return (
    <>
      <div className="settings-section-divider" />
      <div className="settings-section-title">{t('manageSources')}</div>
      <div className="settings-helper settings-mb-12">
        {t('manageSourcesHint')}
      </div>
      {exploreSources.map((src) => (
        <div className="settings-tool-dir-row" key={src.id}>
          <div className="settings-tool-dir-top">
            <span className="settings-tool-dir-label">
              {src.name}
              {src.builtin && (
                <span className="settings-tool-dir-badge">{t('sourceKindFeatured')}</span>
              )}
            </span>
            <div className="settings-tool-dir-actions">
              <label className="settings-switch">
                <input
                  type="checkbox"
                  checked={src.enabled}
                  onChange={() => toggleSource(src.id)}
                />
                <span>{t('sourceEnabled')}</span>
              </label>
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                onClick={() => startEditSource(src)}
              >
                {t('editSource')}
              </button>
              {!src.builtin && (
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  onClick={() => deleteSource(src.id)}
                >
                  {t('deleteSource')}
                </button>
              )}
            </div>
          </div>
          <div className="settings-tool-dir-path mono">{src.endpoint || src.kind}</div>
        </div>
      ))}
      <button
        className="btn btn-primary btn-sm settings-mt-8"
        type="button"
        onClick={startAddSource}
      >
        {t('addSource')}
      </button>

      {sourceEditor && (
        <div className="settings-source-editor">
          <div className="settings-field">
            <label className="settings-label">{t('sourceName')}</label>
            <input
              className="settings-input"
              value={sourceEditor.source.name}
              onChange={(e) =>
                setSourceEditor({
                  ...sourceEditor,
                  source: { ...sourceEditor.source, name: e.target.value },
                })
              }
            />
          </div>
          <div className="settings-field">
            <label className="settings-label">{t('sourceKind')}</label>
            <div className="settings-select-wrap">
              <select
                className="settings-select"
                value={sourceEditor.source.kind}
                onChange={(e) =>
                  setSourceEditor({
                    ...sourceEditor,
                    source: { ...sourceEditor.source, kind: e.target.value },
                  })
                }
              >
                <option value="featured_json">{t('sourceKindFeatured')}</option>
                <option value="skills_sh">{t('sourceKindSkillsSh')}</option>
                <option value="json_index">{t('sourceKindJsonIndex')}</option>
                <option value="git_index">{t('sourceKindGitIndex')}</option>
              </select>
            </div>
          </div>
          <div className="settings-field">
            <label className="settings-label">{t('sourceEndpoint')}</label>
            <input
              className="settings-input mono"
              value={sourceEditor.source.endpoint}
              onChange={(e) =>
                setSourceEditor({
                  ...sourceEditor,
                  source: { ...sourceEditor.source, endpoint: e.target.value },
                })
              }
            />
          </div>
          <div className="settings-field">
            <label className="settings-label">{t('sourceEnabled')}</label>
            <input
              type="checkbox"
              checked={sourceEditor.source.enabled}
              onChange={(e) =>
                setSourceEditor({
                  ...sourceEditor,
                  source: { ...sourceEditor.source, enabled: e.target.checked },
                })
              }
            />
          </div>
          <div className="settings-tool-dir-actions">
            <button className="btn btn-primary btn-sm" type="button" onClick={commitSource}>
              {t('saveSource')}
            </button>
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              onClick={() => setSourceEditor(null)}
            >
              {t('cancel')}
            </button>
          </div>
        </div>
      )}
    </>
  )
})

export default ExploreSourcesSection