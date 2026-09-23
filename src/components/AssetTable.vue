<script lang="ts" setup>
import {computed} from 'vue'
import type {VisualSort, VisualSummary} from '../api/tauri'
import {ArrowDown, ArrowUp, Grid2x2, Image, Loader2, Palette} from 'lucide-vue-next'

const props = defineProps<{
  rows: VisualSummary[]
  /** Selected row, identified by the visual GUID (names are not unique) */
  selected: string | null
  loading: boolean
  /** Active sort column and direction: that header stays lit and carries the arrow */
  sortBy: VisualSort
  sortDesc: boolean
  hideEmpty?: boolean
  /** Show the source column. Only the mod table needs it: every row there is topped by a mod, while
   *  the base-game table is uniformly the game's own and the column would repeat one word */
  showSource?: boolean
}>()

const emit = defineEmits(['select', 'move', 'sort'])

function isActive(id: string) {
  return props.selected === id
}

function isSorted(field: VisualSort) {
  return props.sortBy === field
}

/** Header label colour: the active sort column stays lit, the other one only lights up on hover */
function headerClass(field: VisualSort) {
  return isSorted(field) ? 'text-accent' : 'hover:text-fg'
}

/** Six-column layout: the UUID column leads and caps at 300px (a full 36-char GUID at 12px mono +
 *  padding) but starts at 0, so a narrow window shrinks it to a truncating stub — with a `title`
 *  fallback — instead of pushing the other columns out of the grid; the name column is ≥220px wide
 *  to fit asset names and its 1fr absorbs the remaining space, so the table always fills its
 *  wrapper with zero leftover; the source column only exists where `showSource` is on and is fixed
 *  at 140px, while the three count columns at 80px (14px icon + padding) — they skip fr distribution
 *  for a compact, gap-free fit. */
const GRID_COLS = computed(() =>
    props.showSource
        ? 'minmax(0, 300px) minmax(220px, 1fr) 140px repeat(3, 80px)'
        : 'minmax(0, 300px) minmax(220px, 1fr) repeat(3, 80px)',
)
</script>

<template>
  <div
      class="relative flex w-full min-h-0 flex-1 select-none flex-col overflow-hidden rounded-xl border border-hairline-strong bg-ink-700"
      tabindex="0"
      @keydown.up.prevent="emit('move', -1)"
      @keydown.down.prevent="emit('move', 1)"
  >
    <div
        v-if="loading"
        class="absolute inset-0 z-20 flex items-center justify-center bg-page/70"
    >
      <Loader2 class="h-6 w-6 animate-spin text-accent"/>
    </div>

    <!-- Single scroll surface: when the y axis overflows, only this area scrolls (the sticky header
         stays put) and the page itself never scrolls; x is forced hidden to prevent horizontal scroll -->
    <div class="relative min-h-0 flex-1 overflow-y-auto">
      <div
          :style="{gridTemplateColumns: GRID_COLS}"
          class="sticky top-0 z-10 grid w-full items-center bg-ink-800 text-xs font-semibold text-muted shadow-[inset_0_-1px_0_0_rgb(var(--c-line)_/_var(--alpha-line))]"
      >
        <button
            :aria-label="$t('table.sortById')"
            :class="headerClass('id')"
            :title="$t('table.sortById')"
            class="flex w-full items-center justify-center gap-1 px-4 py-2.5 transition-colors duration-150 ease-fluent"
            type="button"
            @click="emit('sort', 'id')"
        >
          <span class="min-w-0 truncate">{{ $t('table.headerUuid') }}</span>
          <ArrowUp v-if="isSorted('id') && !sortDesc" class="h-3 w-3 shrink-0"/>
          <ArrowDown v-else-if="isSorted('id')" class="h-3 w-3 shrink-0"/>
        </button>
        <button
            :aria-label="$t('table.sortByName')"
            :class="headerClass('name')"
            :title="$t('table.sortByName')"
            class="flex w-full items-center justify-center gap-1 px-4 py-2.5 transition-colors duration-150 ease-fluent"
            type="button"
            @click="emit('sort', 'name')"
        >
          <span class="min-w-0 truncate">{{ $t('table.headerName') }}</span>
          <ArrowUp v-if="isSorted('name') && !sortDesc" class="h-3 w-3 shrink-0"/>
          <ArrowDown v-else-if="isSorted('name')" class="h-3 w-3 shrink-0"/>
        </button>
        <button
            v-if="showSource"
            :aria-label="$t('table.sortBySource')"
            :class="headerClass('source')"
            :title="$t('table.sortBySource')"
            class="flex w-full items-center justify-center gap-1 px-3 py-2.5 transition-colors duration-150 ease-fluent"
            type="button"
            @click="emit('sort', 'source')"
        >
          <span class="min-w-0 truncate">{{ $t('table.headerSource') }}</span>
          <ArrowUp v-if="isSorted('source') && !sortDesc" class="h-3 w-3 shrink-0"/>
          <ArrowDown v-else-if="isSorted('source')" class="h-3 w-3 shrink-0"/>
        </button>
        <div
            :title="$t('table.headerMaterial')"
            class="flex items-center justify-center px-3 py-3"
        >
          <Palette class="h-3.5 w-3.5 text-fg/70"/>
        </div>
        <div
            :title="$t('table.headerTexture')"
            class="flex items-center justify-center px-3 py-3"
        >
          <Image class="h-3.5 w-3.5 text-fg/70"/>
        </div>
        <div
            :title="$t('table.headerVirtual')"
            class="flex items-center justify-center px-3 py-3"
        >
          <Grid2x2 class="h-3.5 w-3.5 text-fg/70"/>
        </div>
      </div>

      <div
          v-for="row in rows"
          :key="row.id"
          :class="isActive(row.id) ? 'bg-tint' : ''"
          :style="{gridTemplateColumns: GRID_COLS}"
          class="relative grid w-full cursor-pointer items-center transition-colors duration-150 ease-fluent shadow-[inset_0_-1px_0_0_rgb(var(--c-line)_/_var(--alpha-line))] last:shadow-none hover:bg-tint"
          @click="emit('select', row.id)"
      >
        <!-- Selection marker on the table's left edge: a Fluent grid marks the selected row with the
             accent bar alone (the row's own fill stays the same subtle hover wash), so the marker is
             what the eye follows. The row box is the containing block, so `left-0` hugs the table's
             far left edge and the rounding faces inward -->
        <span
            v-if="isActive(row.id)"
            class="absolute inset-y-1 left-0 w-[3px] rounded-r-sm bg-accent"
        />
        <div :title="row.id" class="truncate px-4 py-2 text-center font-mono text-[12px] text-muted">
          {{ row.id }}
        </div>
        <div class="truncate px-4 py-2 text-center font-mono text-[13px]">
          <span :class="isActive(row.id) ? 'text-accent' : 'text-fg'">
            {{ row.name }}
          </span>
        </div>
        <!-- Source column: the mod's name as plain text, shown only in the mod table, where every row has
             one -->
        <div
            v-if="showSource"
            :title="row.source"
            class="truncate px-3 py-2 text-center text-[12px] text-fg/85"
        >
          {{ row.source }}
        </div>
        <div
            :title="$t('detail.materialIds', {count: row.materialCount})"
            class="flex items-center justify-center px-3 py-2 font-mono text-[13px] font-medium tabular-nums"
        >
          <span :class="row.materialCount > 0 ? 'text-fg/85' : 'text-faint'">
            {{ row.materialCount }}
          </span>
        </div>
        <div
            :title="$t('detail.textures', {count: row.textureCount})"
            class="flex items-center justify-center px-3 py-2 font-mono text-[13px] font-medium tabular-nums"
        >
          <span :class="row.textureCount > 0 ? 'text-fg/85' : 'text-faint'">
            {{ row.textureCount }}
          </span>
        </div>
        <div
            :title="$t('detail.virtualTextures', {count: row.virtualTextureCount})"
            class="flex items-center justify-center px-3 py-2 font-mono text-[13px] font-medium tabular-nums"
        >
          <span :class="row.virtualTextureCount > 0 ? 'text-fg/85' : 'text-faint'">
            {{ row.virtualTextureCount }}
          </span>
        </div>
      </div>

      <p v-if="!loading && rows.length === 0 && !hideEmpty" class="px-4 py-10 text-center text-sm text-muted">
        {{ $t('table.empty') }}
      </p>
    </div>
  </div>
</template>
