<script lang="ts" setup>
import type {VisualSummary} from '../api/tauri'
import {Grid2x2, Image, Loader2, Palette} from 'lucide-vue-next'

const props = defineProps<{
  rows: VisualSummary[]
  selected: string | null
  loading: boolean
  hideEmpty?: boolean
}>()

const emit = defineEmits(['select', 'move'])

function isActive(name: string) {
  return props.selected === name
}

/** Four-column layout: the name column is ≥220px wide to fit asset names, and 1fr absorbs the
 *  remaining space so the table fills its wrapper; the three count columns are fixed at 80px
 *  (14px icon + padding) — they skip fr distribution for a compact, gap-free fit.
 *  Key: with the numeric columns fixed the table always fills the wrapper with zero leftover and
 *  the name column simply grows as needed */
const GRID_COLS = 'minmax(220px, 1fr) repeat(3, 80px)'
</script>

<template>
  <div
      class="relative flex w-full min-h-0 flex-1 flex-col overflow-hidden rounded-2xl border border-cyan-300/10 bg-ink-700/40 backdrop-blur-xl"
      tabindex="0"
      @keydown.up.prevent="emit('move', -1)"
      @keydown.down.prevent="emit('move', 1)"
  >
    <div
        v-if="loading"
        class="absolute inset-0 z-20 flex items-center justify-center bg-ink-900/60 backdrop-blur-sm"
    >
      <Loader2 class="h-6 w-6 animate-spin text-glow-cyan"/>
    </div>

    <!-- Single scroll surface: when the y axis overflows, only this area scrolls (the sticky header
         stays put) and the page itself never scrolls; x is forced hidden to prevent horizontal scroll -->
    <div class="relative min-h-0 flex-1 overflow-y-auto">
      <div
          :style="{gridTemplateColumns: GRID_COLS}"
          class="sticky top-0 z-10 grid w-full items-center bg-ink-800/95 text-xs uppercase tracking-wider text-muted shadow-[inset_0_-1px_0_0_rgb(255_255_255_/_0.05)]"
      >
        <div class="px-4 py-3 font-medium">{{ $t('table.headerName') }}</div>
        <div
            :title="$t('table.headerMaterial')"
            class="flex items-center justify-center px-3 py-3"
        >
          <Palette class="h-3.5 w-3.5 text-white/70"/>
        </div>
        <div
            :title="$t('table.headerTexture')"
            class="flex items-center justify-center px-3 py-3"
        >
          <Image class="h-3.5 w-3.5 text-white/70"/>
        </div>
        <div
            :title="$t('table.headerVirtual')"
            class="flex items-center justify-center px-3 py-3"
        >
          <Grid2x2 class="h-3.5 w-3.5 text-white/70"/>
        </div>
      </div>

      <div
          v-for="row in rows"
          :key="row.name"
          :class="isActive(row.name) ? 'bg-glow-cyan/10' : ''"
          :style="{gridTemplateColumns: GRID_COLS}"
          class="grid w-full cursor-pointer items-center transition-colors shadow-[inset_0_-1px_0_0_rgb(255_255_255_/_0.05)] last:shadow-none hover:bg-white/5"
          @click="emit('select', row.name)"
      >
        <div class="relative truncate px-4 py-2 font-mono text-[13px]">
          <span
              v-if="isActive(row.name)"
              class="absolute inset-y-1 left-0 w-[3px] rounded-r bg-glow-cyan"
          />
          <span :class="isActive(row.name) ? 'text-glow-cyan' : 'text-[#E6EDF7]'">
            {{ row.name }}
          </span>
        </div>
        <div
            :title="$t('detail.materialIds', {count: row.materialCount})"
            class="flex items-center justify-center px-3 py-2 font-mono text-[13px] font-medium tabular-nums"
        >
          <span :class="row.materialCount > 0 ? 'text-[#E6EDF7]/85' : 'text-white/15'">
            {{ row.materialCount }}
          </span>
        </div>
        <div
            :title="$t('detail.textures', {count: row.textureCount})"
            class="flex items-center justify-center px-3 py-2 font-mono text-[13px] font-medium tabular-nums"
        >
          <span :class="row.textureCount > 0 ? 'text-[#E6EDF7]/85' : 'text-white/15'">
            {{ row.textureCount }}
          </span>
        </div>
        <div
            :title="$t('detail.virtualTextures', {count: row.virtualTextureCount})"
            class="flex items-center justify-center px-3 py-2 font-mono text-[13px] font-medium tabular-nums"
        >
          <span :class="row.virtualTextureCount > 0 ? 'text-[#E6EDF7]/85' : 'text-white/15'">
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
