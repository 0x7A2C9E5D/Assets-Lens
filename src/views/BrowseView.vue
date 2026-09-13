<script lang="ts" setup>
import {computed, onMounted, onUnmounted, ref, watch} from 'vue'
import {useRouter} from 'vue-router'
import {useI18n} from 'vue-i18n'
import {Database, Search, X} from 'lucide-vue-next'
import AssetDetail from '../components/AssetDetail.vue'
import AssetTable from '../components/AssetTable.vue'
import EmptyState from '../components/EmptyState.vue'
import PaginationBar from '../components/PaginationBar.vue'
import {
  type DatabaseStats,
  dbStats,
  getVisual,
  listVisuals,
  type VisualAsset,
  type VisualSort,
  type VisualSummary,
} from '../api/tauri'

const router = useRouter()
const {t} = useI18n()
const limit = ref(20)
const offset = ref(0)
const total = ref(0)
const rows = ref<VisualSummary[]>([])
/** Selected visual GUID: the list is not deduplicated by name, so the name cannot identify a row */
const selected = ref<string | null>(null)
const asset = ref<VisualAsset | null>(null)
const loading = ref(false)
const detailLoading = ref(false)
const errorMsg = ref('')
const stats = ref<DatabaseStats | null>(null)
/** The list is only worth loading once the backend reports a built database */
const ready = computed(() => stats.value !== null)

/** Sort state: the backend owns the order (paging happens there), the header just picks a column */
const sortBy = ref<VisualSort>('name')
const sortDesc = ref(false)

const searchTerm = ref('')
const searchActive = computed(() => searchTerm.value.trim().length > 0)
let searchTimer: ReturnType<typeof setTimeout> | null = null

function load() {
  loading.value = true
  listVisuals(offset.value, limit.value, searchTerm.value, sortBy.value, sortDesc.value)
      .then((page) => {
        rows.value = page.items
        total.value = page.total
        offset.value = page.offset
      })
      .catch((err) => {
        console.error('[list_visuals]', err)
        errorMsg.value = t('errors.loadList')
      })
      .finally(() => {
        loading.value = false
      })
}

function selectRow(id: string) {
  // Clicking the visual that is already selected (or arrowing back onto it) is not a new request:
  // the panel shows that asset already and a repeat fetch would only flash the loading state while
  // re-running an expensive lookup, so it is skipped
  if (selected.value === id) return

  selected.value = id
  asset.value = null
  detailLoading.value = true

  getVisual(id)
      .then((result) => {
        asset.value = result
      })
      .catch((err) => {
        console.error('[get_visual]', err)
        errorMsg.value = t('errors.loadDetail')
      })
      .finally(() => {
        detailLoading.value = false
      })
}

function moveSelection(delta: number) {
  if (!rows.value.length) return
  const ids = rows.value.map((row) => row.id)
  const index = ids.indexOf(selected.value ?? '')
  const next = index === -1 ? 0 : Math.min(Math.max(index + delta, 0), ids.length - 1)
  selectRow(ids[next])
}

/** Header click: the active column flips direction, another column starts ascending again */
function toggleSort(field: VisualSort) {
  if (sortBy.value === field) {
    sortDesc.value = !sortDesc.value
  } else {
    sortBy.value = field
    sortDesc.value = false
  }
  offset.value = 0
  load()
}

function changePage(nextOffset: number) {
  offset.value = nextOffset
  load()
}

function clearSearch() {
  searchTerm.value = ''
}

watch(searchTerm, () => {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => {
    offset.value = 0
    selected.value = null
    asset.value = null
    load()
  }, 200)
})

onUnmounted(() => {
  if (searchTimer) clearTimeout(searchTimer)
})

onMounted(() => {
  dbStats()
      .then((result) => {
        stats.value = result
        if (ready.value) load()
      })
      .catch((err) => console.error('[db_stats]', err))
})
</script>

<template>
  <!-- Full-page responsive layout: the section stretches with the main area; long lists scroll only inside the table area -->
  <section class="flex min-h-0 w-full flex-1 flex-col gap-4 px-8 pt-3 pb-4">
    <header class="flex shrink-0 flex-wrap items-start justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold tracking-wide text-[#E6EDF7]">
          {{ $t('browse.title') }}
        </h1>
        <p class="mt-1 text-sm text-muted">{{ $t('browse.subtitle') }}</p>
      </div>
      <div class="relative w-full self-end lg:w-[380px]">
        <Search class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted"/>
        <input
            v-model="searchTerm"
            :placeholder="$t('browse.searchPlaceholder')"
            autocomplete="off"
            class="w-full rounded-xl border border-white/5 bg-ink-900/50 py-2 pl-9 pr-9 text-sm text-[#E6EDF7] placeholder:text-muted/60 transition-colors focus:border-cyan-300/30 focus:outline-none"
            spellcheck="false"
            type="text"
        />
        <button
            v-if="searchActive"
            :aria-label="$t('browse.searchClearAria')"
            :title="$t('browse.searchClearAria')"
            class="absolute right-2 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted transition-colors hover:text-[#E6EDF7]"
            type="button"
            @click="clearSearch"
        >
          <X class="h-3.5 w-3.5"/>
        </button>
      </div>
    </header>

    <EmptyState
        v-if="!ready"
        :action-label="$t('browse.emptyAction')"
        :description="$t('browse.emptyDescription')"
        :icon="Database"
        :title="$t('browse.emptyTitle')"
        @action="router.push('/database')"
    />

    <EmptyState
        v-else-if="searchActive && !loading && rows.length === 0"
        :action-label="$t('browse.searchClearAction')"
        :icon="Search"
        :title="$t('browse.searchEmpty', {term: searchTerm})"
        @action="clearSearch"
    />

    <template v-else>
      <div
          v-if="errorMsg"
          class="flex items-start gap-3 rounded-2xl border border-red-400/30 bg-red-500/10 px-4 py-3 text-sm text-red-200"
      >
        <Search class="mt-0.5 h-4 w-4 shrink-0"/>
        <span class="break-all">{{ errorMsg }}</span>
      </div>

      <!-- Fixed two columns: list on the left, detail on the right; the list fills the area and scrolls internally -->
      <div class="flex min-h-0 flex-1 gap-4">
        <div class="flex min-h-0 min-w-0 flex-1 flex-col">
          <AssetTable
              :hide-empty="searchActive"
              :loading="loading"
              :rows="rows"
              :selected="selected"
              :sort-by="sortBy"
              :sort-desc="sortDesc"
              @move="moveSelection"
              @select="selectRow"
              @sort="toggleSort"
          />
        </div>

        <div class="flex min-h-0 w-[380px] shrink-0 flex-col">
          <AssetDetail :asset="asset" :loading="detailLoading"/>
        </div>
      </div>

      <!-- Pagination bar sits as a section-level footer, spanning full width and aligned with the detail panel's right edge -->
      <PaginationBar :limit="limit" :offset="offset" :total="total" @change="changePage"/>
    </template>
  </section>
</template>
