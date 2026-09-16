<script lang="ts" setup>
import {useI18n} from 'vue-i18n'

/** Icon colour. Fluent keeps colour scarce: one plate stays neutral so the other three can carry
 *  the brand / status ramps without the row turning into four competing accents. */
type Accent = 'brand' | 'success' | 'warning' | 'neutral'

const props = defineProps<{
  label: string
  value: number | string
  hint: string
  icon: unknown
  accent?: Accent
}>()

const {locale} = useI18n()

function display(value: number | string) {
  return typeof value === 'number' ? value.toLocaleString(locale.value) : value
}

const accentClass: Record<Accent, string> = {
  brand: 'text-accent',
  success: 'text-success',
  warning: 'text-warning',
  neutral: 'text-muted',
}
</script>

<template>
  <div class="card p-5">
    <!-- The icon centers against the text block: it is the only thing on its side, so the row's
         cross axis puts it halfway down the label / value / hint stack -->
    <div class="flex items-center justify-between gap-3">
      <div class="min-w-0">
        <p class="text-xs font-semibold text-muted">{{ label }}</p>
        <!-- Display optical size, tabular figures: Fluent sets headline numbers in the same family
             as the copy, so a counting value does not jump when its digits change -->
        <p class="mt-1.5 font-display text-3xl font-semibold tabular-nums text-fg">
          {{ display(props.value) }}
        </p>
        <p class="mt-1 text-xs text-subtle">{{ hint }}</p>
      </div>
      <!-- Bare glyph at the plate's own 40px. Lucide scales its 24-unit artwork up to this size, which
           thickens the 2-unit default stroke on screen — so the stroke is dialled down to keep the
           drawn weight the same as an icon at its native size -->
      <component
          :class="accentClass[accent ?? 'brand']"
          :is="icon"
          class="h-10 w-10 shrink-0 [stroke-width:1.5]"
      />
    </div>
  </div>
</template>
