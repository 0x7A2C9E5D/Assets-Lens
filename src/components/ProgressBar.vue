<script lang="ts" setup>
import {computed} from 'vue'

const props = defineProps<{ percent: number }>()

const width = computed(() => `${Math.min(100, Math.max(0, props.percent * 100)).toFixed(1)}%`)
</script>

<template>
  <div class="space-y-2">
    <div class="flex items-center justify-between text-xs text-muted">
      <span class="truncate font-mono">{{ $t('progress.working') }}</span>
      <span class="shrink-0 font-mono">{{ width }}</span>
    </div>
    <div class="relative h-2.5 w-full overflow-hidden rounded-full bg-ink-900/80">
      <div
          :style="{ width }"
          class="h-full rounded-full bg-gradient-to-r from-glow-cyan to-glow-blue transition-[width] duration-200"
      />
      <div
          class="pointer-events-none absolute inset-y-0 left-0 w-1/3 animate-shimmer bg-gradient-to-r from-transparent via-white/25 to-transparent"
      />
    </div>
  </div>
</template>
