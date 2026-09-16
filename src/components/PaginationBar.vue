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

function go(target: number) {
  const next = Math.min(Math.max(target, 0), (pageCount.value - 1) * props.limit)
  if (next !== props.offset) emit('change', next)
}

function jump(delta: number) {
  go(props.offset + delta * props.limit)
}
</script>

<template>
  <div class="grid grid-cols-3 items-center gap-3 pt-3">
    <p class="justify-self-start text-xs text-muted">{{ rangeText }}</p>
    <div class="flex items-center justify-center gap-1">
      <button
          :disabled="offset === 0"
          :title="$t('pagination.first')"
          class="btn-ghost !h-6 !w-6 !px-0"
          type="button"
          @click="go(0)"
      >
        <ChevronsLeft class="h-3.5 w-3.5"/>
      </button>
      <button
          :disabled="offset === 0"
          :title="$t('pagination.prev')"
          class="btn-ghost !h-6 !w-6 !px-0"
          type="button"
          @click="jump(-1)"
      >
        <ChevronLeft class="h-3.5 w-3.5"/>
      </button>
      <span
          class="flex h-6 items-center justify-center px-3 text-sm font-semibold tabular-nums text-fg"
      >
        {{ page }} / {{ pageCount }}
      </span>
      <button
          :disabled="offset + limit >= total"
          :title="$t('pagination.next')"
          class="btn-ghost !h-6 !w-6 !px-0"
          type="button"
          @click="jump(1)"
      >
        <ChevronRight class="h-3.5 w-3.5"/>
      </button>
      <button
          :disabled="offset + limit >= total"
          :title="$t('pagination.last')"
          class="btn-ghost !h-6 !w-6 !px-0"
          type="button"
          @click="go((pageCount - 1) * limit)"
      >
        <ChevronsRight class="h-3.5 w-3.5"/>
      </button>
    </div>
  </div>
</template>
