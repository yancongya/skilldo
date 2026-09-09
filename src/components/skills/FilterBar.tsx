import { memo, useEffect, useMemo, useRef, useState } from 'react'
import { ArrowUpDown, Check, ChevronDown, LayoutGrid, LayoutList, RefreshCw, Search, SlidersHorizontal } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { TagWithCountDto } from './types'

type FilterBarProps = {
  sortBy: 'updated' | 'name'
  searchQuery: string
  scopeFilter: 'all' | 'global' | 'project'
  sourceFilter: 'all' | 'syncable' | 'local'
  viewMode: 'list' | 'grid'
  tags: TagWithCountDto[]
  selectedTagIds: number[]
  includeUntagged: boolean
  untaggedCount: number
  totalCount: number
  pendingUpdateCount: number
  checkingUpdates: boolean
  onSortChange: (value: 'updated' | 'name') => void
  onSearchChange: (value: string) => void
  onScopeFilterChange: (value: 'all' | 'global' | 'project') => void
  onSourceFilterChange: (value: 'all' | 'syncable' | 'local') => void
  onViewModeChange: (mode: 'list' | 'grid') => void
  onCheckUpdates: () => void
  onToggleTag: (tagId: number) => void
  onToggleUntagged: () => void
  onClearTags: () => void
  onManageTags: () => void
  t: TFunction
}

const FilterBar = ({
  sortBy,
  searchQuery,
  scopeFilter,
  sourceFilter,
  viewMode,
  tags,
  selectedTagIds,
  includeUntagged,
  untaggedCount,
  totalCount,
  pendingUpdateCount,
  checkingUpdates,
  onSortChange,
  onSearchChange,
  onScopeFilterChange,
  onSourceFilterChange,
  onViewModeChange,
  onCheckUpdates,
  onToggleTag,
  onToggleUntagged,
  onClearTags,
  onManageTags,
  t,
}: FilterBarProps) => {
  const [filterMenuOpen, setFilterMenuOpen] = useState(false)
  const [tagQuery, setTagQuery] = useState('')
  const filterMenuRef = useRef<HTMLDivElement | null>(null)
  const scopeOptions: { value: 'all' | 'global' | 'project'; label: string }[] = [
    { value: 'all', label: t('scope.all') },
    { value: 'global', label: t('scope.global') },
    { value: 'project', label: t('scope.project') },
  ]
  const sourceOptions: { value: 'all' | 'syncable' | 'local'; label: string }[] = [
    { value: 'all', label: t('filterAll') },
    { value: 'syncable', label: t('filterSyncable') },
    { value: 'local', label: t('filterLocal') },
  ]
  const selectedTagSet = useMemo(() => new Set(selectedTagIds), [selectedTagIds])
  const selectedCount = selectedTagIds.length + (includeUntagged ? 1 : 0)
  const activeFilterCount = selectedCount + Number(scopeFilter !== 'all') + Number(sourceFilter !== 'all')
  const filteredTags = useMemo(() => {
    const query = tagQuery.trim().toLowerCase()
    if (!query) return tags
    return tags.filter((tag) => tag.name.toLowerCase().includes(query))
  }, [tagQuery, tags])

  useEffect(() => {
    if (!filterMenuOpen) return
    const handlePointerDown = (event: MouseEvent) => {
      if (!filterMenuRef.current?.contains(event.target as Node)) {
        setFilterMenuOpen(false)
      }
    }
    document.addEventListener('mousedown', handlePointerDown)
    return () => document.removeEventListener('mousedown', handlePointerDown)
  }, [filterMenuOpen])

  return (
    <div className="filter-bar">
      <div className="filter-title">
        {t('allSkills')}（{totalCount}）
      </div>
      <div className="filter-actions">
        <button
          className={`btn btn-secondary check-skills-update-btn${pendingUpdateCount > 0 ? ' active' : ''}`}
          type="button"
          onClick={onCheckUpdates}
          disabled={checkingUpdates}
          title={t('skillsUpdateCheckHint')}
        >
          <RefreshCw size={14} />
          {checkingUpdates
            ? t('checkingUpdates')
            : pendingUpdateCount > 0
              ? t('skillsUpdatesFound', { count: pendingUpdateCount })
              : t('checkSkillUpdates')}
        </button>
        <div className="tag-filter-wrap" ref={filterMenuRef}>
          <button
            className={`btn btn-secondary tag-filter-btn${activeFilterCount > 0 ? ' active' : ''}`}
            type="button"
            onClick={() => setFilterMenuOpen((open) => !open)}
            aria-expanded={filterMenuOpen}
          >
            <SlidersHorizontal size={14} />
            {activeFilterCount > 0 ? t('filtersActive', { count: activeFilterCount }) : t('filters')}
            <ChevronDown size={12} />
          </button>
          {filterMenuOpen ? (
            <div className="tag-filter-menu combined-filter-menu">
              <div className="combined-filter-section">
                <label htmlFor="skills-scope-filter">{t('scope.filterLabel')}</label>
                <select
                  id="skills-scope-filter"
                  value={scopeFilter}
                  onChange={(event) => onScopeFilterChange(event.target.value as 'all' | 'global' | 'project')}
                >
                  {scopeOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
                </select>
              </div>
              <div className="combined-filter-section">
                <label htmlFor="skills-source-filter">{t('source.filterLabel')}</label>
                <select
                  id="skills-source-filter"
                  value={sourceFilter}
                  onChange={(event) => onSourceFilterChange(event.target.value as 'all' | 'syncable' | 'local')}
                >
                  {sourceOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
                </select>
              </div>
              <div className="tag-filter-head">
                <span>{t('tags')}</span>
                <span>{t('matchAny')}</span>
              </div>
              <div className="tag-filter-search">
                <Search size={15} />
                <input
                  value={tagQuery}
                  onChange={(event) => setTagQuery(event.target.value)}
                  placeholder={t('searchTags')}
                />
              </div>
              <div className="tag-filter-options">
                <button className={`tag-filter-option${includeUntagged ? ' selected' : ''}`} type="button" onClick={onToggleUntagged}>
                  <span className="tag-check">{includeUntagged ? <Check size={14} /> : null}</span>
                  <span>{t('untagged')}</span><span className="tag-count">{untaggedCount}</span>
                </button>
                {filteredTags.map((tag) => {
                  const selected = selectedTagSet.has(tag.id)
                  return <button key={tag.id} className={`tag-filter-option${selected ? ' selected' : ''}`} type="button" onClick={() => onToggleTag(tag.id)}>
                    <span className="tag-check">{selected ? <Check size={14} /> : null}</span>
                    <span>{tag.name}</span><span className="tag-count">{tag.skill_count}</span>
                  </button>
                })}
              </div>
              <div className="tag-filter-footer">
                <button type="button" onClick={onClearTags} disabled={selectedCount === 0}>{t('clearAll')}</button>
                <button type="button" onClick={onManageTags}>{t('manageTags')}</button>
              </div>
            </div>
          ) : null}
        </div>
        <button className="btn btn-secondary sort-btn" type="button">
          {sortBy === 'updated' ? t('sortUpdated') : t('sortName')}
          <ArrowUpDown size={12} />
          <select
            aria-label={t('filterSort')}
            value={sortBy}
            onChange={(event) => onSortChange(event.target.value as 'updated' | 'name')}
          >
            <option value="updated">{t('sortUpdated')}</option>
            <option value="name">{t('sortName')}</option>
          </select>
        </button>
        <div className="filter-view-toggle">
          <button
            className={`icon-btn${viewMode === 'list' ? ' active' : ''}`}
            type="button"
            onClick={() => onViewModeChange('list')}
            title={t('switchToList')}
          >
            <LayoutList size={16} />
          </button>
          <button
            className={`icon-btn${viewMode === 'grid' ? ' active' : ''}`}
            type="button"
            onClick={() => onViewModeChange('grid')}
            title={t('switchToGrid')}
          >
            <LayoutGrid size={16} />
          </button>
        </div>
        <div className="search-container">
          <Search size={16} className="search-icon-abs" />
          <input
            className="search-input"
            value={searchQuery}
            onChange={(event) => onSearchChange(event.target.value)}
            placeholder={t('searchPlaceholder')}
          />
        </div>
      </div>
    </div>
  )
}

export default memo(FilterBar)
