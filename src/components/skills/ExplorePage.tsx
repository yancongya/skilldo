import { memo, useMemo, useState } from 'react'
import { Database, Plus, Search, Settings2, Star, X } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { ExploreSkillDto, ExploreSourceConfigDto, ManagedSkill } from './types'

type ExplorePageProps = {
  skills: ExploreSkillDto[]
  sources: ExploreSourceConfigDto[]
  loadingSources: boolean
  exploreLoading: boolean
  exploreFilter: string
  managedSkills: ManagedSkill[]
  loading: boolean
  onExploreFilterChange: (value: string) => void
  onSaveSources: (sources: ExploreSourceConfigDto[]) => void
  onInstallSkill: (sourceUrl: string, skillName?: string) => void
  onOpenManualAdd: () => void
  t: TFunction
}

function formatCount(n: number): string {
  if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`
  if (n >= 1000) return `${(n / 1000).toFixed(1)}K`
  return String(n)
}

function normalizeSource(source: string): string {
  return source
    .replace('https://github.com/', '')
    .replace('http://github.com/', '')
    .replace(/\.git$/, '')
    .split('/tree/')[0]
    .toLowerCase()
}

const ExplorePage = ({
  skills,
  sources,
  loadingSources,
  exploreLoading,
  exploreFilter,
  managedSkills,
  loading,
  onExploreFilterChange,
  onSaveSources,
  onInstallSkill,
  onOpenManualAdd,
  t,
}: ExplorePageProps) => {
  const [sourcePanelOpen, setSourcePanelOpen] = useState(false)
  const [draftSources, setDraftSources] = useState<ExploreSourceConfigDto[]>([])

  const installedSkillKeys = useMemo(() => {
    const keys = new Set<string>()
    for (const skill of managedSkills) {
      keys.add(`${skill.name.toLowerCase()}|${normalizeSource(skill.source_ref ?? '')}`)
    }
    return keys
  }, [managedSkills])

  const enabledSourceCount = sources.filter((source) => source.enabled).length

  const openSourcePanel = () => {
    setDraftSources(sources)
    setSourcePanelOpen(true)
  }

  const closeSourcePanel = () => {
    if (loadingSources) return
    setSourcePanelOpen(false)
  }

  const updateDraftSource = (
    id: string,
    patch: Partial<ExploreSourceConfigDto>,
  ) => {
    setDraftSources((current) =>
      current.map((source) => (source.id === id ? { ...source, ...patch } : source)),
    )
  }

  const addPrivateSource = () => {
    const id = `custom-${Date.now()}`
    setDraftSources((current) => [
      ...current,
      {
        id,
        name: t('exploreSourceCustomName'),
        kind: 'json_index',
        endpoint: '',
        enabled: true,
        builtin: false,
      },
    ])
  }

  const removeDraftSource = (id: string) => {
    setDraftSources((current) => current.filter((source) => source.id !== id))
  }

  const saveSources = () => {
    onSaveSources(draftSources)
    setSourcePanelOpen(false)
  }

  const isInstalled = (skill: ExploreSkillDto) =>
    installedSkillKeys.has(`${skill.name.toLowerCase()}|${normalizeSource(skill.sourceUrl)}`)

  return (
    <div className="explore-page">
      <div className="explore-hero">
        <div className="explore-search-row">
          <div className="explore-search-wrap">
            <Search size={16} className="explore-search-icon" />
            <input
              className="explore-search-input"
              placeholder={t('exploreFilterPlaceholder')}
              value={exploreFilter}
              onChange={(e) => onExploreFilterChange(e.target.value)}
            />
          </div>
          <button
            className="btn btn-secondary explore-manual-btn"
            type="button"
            onClick={openSourcePanel}
            disabled={loading}
            title={t('exploreSourcesTitle')}
          >
            <Settings2 size={15} />
            {t('exploreSourcesButton', { count: enabledSourceCount })}
          </button>
          <button
            className="btn btn-secondary explore-manual-btn"
            type="button"
            onClick={onOpenManualAdd}
            disabled={loading}
          >
            <Plus size={15} />
            {t('manualAdd')}
          </button>
        </div>
        <div className="explore-source-label">{t('exploreSourceHint')}</div>
      </div>

      <div className="explore-scroll">
        {exploreLoading ? (
          <div className="explore-loading">{t('exploreLoading')}</div>
        ) : skills.length > 0 ? (
          <>
            <div className="explore-section-title">
              {exploreFilter.trim() ? t('exploreSearchResultsTitle') : t('exploreFeaturedTitle')}
            </div>
            <div className="explore-grid">
              {skills.map((skill) => {
                const installed = isInstalled(skill)
                return (
                  <div key={skill.id} className="explore-card">
                    <div className="explore-card-top">
                      <div className="explore-card-info">
                        <div className="explore-card-name">{skill.name}</div>
                        <div className="explore-card-author">
                          {skill.sourceName} · {normalizeSource(skill.sourceUrl)}
                        </div>
                      </div>
                      {installed ? (
                        <span className="explore-btn-installed">{t('status.installed')}</span>
                      ) : (
                        <button
                          className="explore-btn-install"
                          type="button"
                          disabled={loading}
                          onClick={() => onInstallSkill(skill.sourceUrl, skill.name)}
                        >
                          {t('install')}
                        </button>
                      )}
                    </div>
                    {skill.summary ? (
                      <div className="explore-card-desc">{skill.summary}</div>
                    ) : null}
                    <div className="explore-card-bottom">
                      <div className="explore-card-stats">
                        {skill.stars > 0 ? (
                          <span className="explore-stat">
                            <Star size={12} />
                            {formatCount(skill.stars)}
                          </span>
                        ) : null}
                        {skill.downloads > 0 ? (
                          <span className="explore-stat">
                            <Database size={12} />
                            {formatCount(skill.downloads)}
                          </span>
                        ) : null}
                        <span className="explore-source-chip">{skill.sourceKind}</span>
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>
          </>
        ) : (
          <div className="explore-empty">
            {exploreFilter.trim() ? t('searchEmpty') : t('exploreEmpty')}
          </div>
        )}
      </div>

      {sourcePanelOpen ? (
        <div className="modal-backdrop" onClick={closeSourcePanel}>
          <section
            className="modal modal-lg explore-sources-modal"
            role="dialog"
            aria-modal="true"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="modal-header">
              <div>
                <div className="modal-title">{t('exploreSourcesTitle')}</div>
                <div className="modal-subtitle">{t('exploreSourcesSubtitle')}</div>
              </div>
              <button
                className="modal-close"
                type="button"
                onClick={closeSourcePanel}
                disabled={loadingSources}
                aria-label={t('close')}
              >
                <X size={16} />
              </button>
            </div>
            <div className="modal-body explore-sources-body">
              {draftSources.map((source) => (
                <div className="explore-source-row" key={source.id}>
                  <label className="settings-toggle-row">
                    <button
                      className={`settings-toggle${source.enabled ? ' checked' : ''}`}
                      type="button"
                      onClick={() => updateDraftSource(source.id, { enabled: !source.enabled })}
                      aria-pressed={source.enabled}
                    >
                      <span className="settings-toggle-knob" />
                    </button>
                  </label>
                  <div className="explore-source-fields">
                    <div className="settings-input-row">
                      <input
                        className="settings-input"
                        value={source.name}
                        onChange={(event) =>
                          updateDraftSource(source.id, { name: event.target.value })
                        }
                        placeholder={t('exploreSourceName')}
                      />
                      <select
                        className="settings-select explore-source-kind"
                        value={source.kind}
                        onChange={(event) =>
                          updateDraftSource(source.id, { kind: event.target.value })
                        }
                      >
                        <option value="featured_json">featured_json</option>
                        <option value="skills_sh">skills_sh</option>
                        <option value="json_index">json_index</option>
                        <option value="git_index">git_index</option>
                      </select>
                    </div>
                    <input
                      className="settings-input mono"
                      value={source.endpoint}
                      onChange={(event) =>
                        updateDraftSource(source.id, { endpoint: event.target.value })
                      }
                      placeholder={t('exploreSourceEndpoint')}
                    />
                    <div className="settings-helper">
                      {source.builtin
                        ? t('exploreSourceBuiltinHint')
                        : t('exploreSourceCustomHint')}
                    </div>
                  </div>
                  {!source.builtin ? (
                    <button
                      className="btn btn-secondary btn-sm"
                      type="button"
                      onClick={() => removeDraftSource(source.id)}
                      disabled={loadingSources}
                    >
                      {t('remove')}
                    </button>
                  ) : null}
                </div>
              ))}
              <button
                className="btn btn-secondary"
                type="button"
                onClick={addPrivateSource}
                disabled={loadingSources}
              >
                <Plus size={15} />
                {t('exploreAddSource')}
              </button>
            </div>
            <div className="modal-footer space-between">
              <button
                className="btn btn-secondary"
                type="button"
                onClick={closeSourcePanel}
                disabled={loadingSources}
              >
                {t('close')}
              </button>
              <button
                className="btn btn-primary"
                type="button"
                onClick={saveSources}
                disabled={loadingSources}
              >
                {t('save')}
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </div>
  )
}

export default memo(ExplorePage)
