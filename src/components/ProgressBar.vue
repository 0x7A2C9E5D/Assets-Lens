<script lang="ts" setup>
import {computed} from 'vue'

const props = defineProps<{ percent: number }>()

const width = computed(() => `${Math.min(100, Math.max(0, props.percent * 100)).toFixed(1)}%`)
</script>

<template>
  <div class="space-y-2">
    <div class="flex items-center justify-between text-xs text-muted">
      <span class="flex min-w-0 items-center gap-2 truncate font-mono">
        <slot>{{ $t('progress.working') }}</slot>
      </span>
      <span class="shrink-0 font-mono">{{ width }}</span>
    </div>
    <div class="relative h-2.5 w-full overflow-hidden rounded-full bg-ink-900">
      <div
          :style="{ width }"
          class="h-full rounded-full bg-gradient-to-r from-glow-cyan to-glow-blue transition-[width] duration-200"
      />
      <div
          class="pointer-events-none absolute inset-y-0 left-0 w-1/3 animate-shimmer bg-gradient-to-r from-transparent via-overlay/25 to-transparent"
      />
    </div>
  </div>
</template>
