<script lang="ts" setup>
import {computed} from 'vue'
import {useI18n} from 'vue-i18n'
import {ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight} from 'lucide-vue-next'

const props = defineProps<{ offset: number; total: number; limit: number }>()
const emit = defineEmits<{ change: [offset: number] }>()

const {t, locale} = useI18n()

const page = computed(() => Math.floor(props.offset / props.limit) + 1)
const pageCount = computed(() => Math.max(1, Math.ceil(props.total / props.limit)))
const rangeText = computed(() => {
  if (props.total === 0) return t('pagination.rangeEmpty')
  const from = props.offset + 1
  const to = Math.min(props.offset + props.limit, props.total)
  return t('pagination.range', {from, to, total: props.total.toLocaleString(locale.value)})
})

/** At most ten page buttons; the window slides with the current page and the gaps become ellipses */
const WINDOW = 10
const pages = computed(() => {
  const last = pageCount.value
  if (last <= WINDOW) {
    return {items: Array.from({length: last}, (_, i) => i + 1), before: false, after: false}
  }
  const start = Math.min(Math.max(page.value - Math.floor(WINDOW / 2), 1), last - WINDOW + 1)
  return {
    items: Array.from({length: WINDOW}, (_, i) => start + i),
    before: start > 1,
    after: start + WINDOW - 1 < last,
  }
})

function go(target: number) {
  const next = Math.min(Math.max(target, 0), (pageCount.value - 1) * props.limit)
  if (next !== props.offset) emit('change', next)
}

function jump(delta: number) {
  go(props.offset + delta * props.limit)
}

function goPage(target: number) {
  go((target - 1) * props.limit)
}
</script>

<template>
  <div class="grid grid-cols-3 items-center gap-3 pt-3">
    <p class="justify-self-start text-xs text-muted">{{ rangeText }}</p>
    <nav
        :aria-label="rangeText"
        class="pager flex items-center justify-self-center gap-0.5 rounded-full border border-hairline-strong bg-page p-1"
    >
      <button
          :disabled="offset === 0"
          :aria-label="$t('pagination.first')"
          class="pager-step"
          type="button"
          @click="go(0)"
      >
        <ChevronsLeft class="h-3.5 w-3.5"/>
      </button>
      <button
          :disabled="offset === 0"
          :aria-label="$t('pagination.prev')"
          class="pager-step"
          type="button"
          @click="jump(-1)"
      >
        <ChevronLeft class="h-3.5 w-3.5"/>
      </button>
      <span v-if="pages.before" class="pager-gap">…</span>
      <button
          v-for="item in pages.items"
          :key="item"
          :aria-current="item === page ? 'page' : undefined"
          :class="['pager-page', item === page && 'pager-page-active']"
          type="button"
          @click="goPage(item)"
      >
        {{ item }}
      </button>
      <span v-if="pages.after" class="pager-gap">…</span>
      <button
          :disabled="offset + limit >= total"
          :aria-label="$t('pagination.next')"
          class="pager-step"
          type="button"
          @click="jump(1)"
      >
        <ChevronRight class="h-3.5 w-3.5"/>
      </button>
      <button
          :disabled="offset + limit >= total"
          :aria-label="$t('pagination.last')"
          class="pager-step"
          type="button"
          @click="go((pageCount - 1) * limit)"
      >
        <ChevronsRight class="h-3.5 w-3.5"/>
      </button>
    </nav>
  </div>
</template>

<style scoped>
/* The pager reads as one pill: a step control at each end, compact numbered pages in between, and
   only the current page carrying a filled surface. */
.pager {
  box-shadow: var(--shadow-card);
}

.pager-step {
  @apply flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-muted transition-colors
  duration-150 ease-fluent hover:bg-tint hover:text-fg disabled:cursor-not-allowed disabled:opacity-30
  disabled:hover:bg-transparent disabled:hover:text-muted;
}

.pager-page {
  @apply flex h-6 min-w-[1.5rem] shrink-0 items-center justify-center rounded-full px-1 text-xs
  font-medium tabular-nums text-muted transition-colors duration-150 ease-fluent hover:bg-tint
  hover:text-fg;
}

.pager-page-active {
  @apply bg-ink-700 font-semibold text-fg hover:bg-ink-700;
}

.pager-gap {
  @apply flex h-6 w-4 shrink-0 items-center justify-center text-xs text-muted;
}
</style>
