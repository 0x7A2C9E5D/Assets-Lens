<script lang="ts" setup>
import {computed} from 'vue'

const props = defineProps<{ percent: number }>()

const width = computed(() => `${Math.min(100, Math.max(0, props.percent * 100)).toFixed(1)}%`)
</script>

<template>
  <div class="space-y-2">
    <div class="flex items-center justify-between gap-3 text-xs text-muted">
      <span class="flex min-w-0 items-center gap-2 truncate">
        <slot>{{ $t('progress.working') }}</slot>
      </span>
      <!-- Tabular figures so the percentage does not shift the row while it counts -->
      <span class="shrink-0 tabular-nums">{{ width }}</span>
    </div>
    <!-- Fluent ProgressBar: a neutral track with a solid brand fill and no shimmer ornament -->
    <div class="h-1.5 w-full overflow-hidden rounded-full bg-ink-900">
      <div
          :style="{ width }"
          class="h-full rounded-full bg-accent transition-[width] duration-200 ease-fluent"
      />
    </div>
  </div>
</template>
