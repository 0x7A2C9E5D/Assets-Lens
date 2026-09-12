<script lang="ts" setup>
import {useI18n} from 'vue-i18n'

type Accent = 'cyan' | 'gold' | 'blue' | 'green'

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
  cyan: 'from-glow-cyan/25 to-transparent text-glow-cyan',
  gold: 'from-glow-gold/25 to-transparent text-glow-gold',
  blue: 'from-glow-blue/25 to-transparent text-glow-blue',
  green: 'from-emerald-400/25 to-transparent text-emerald-300',
}
</script>

<template>
  <div class="glass-card overflow-hidden p-5">
    <div
        :class="accentClass[accent ?? 'cyan']"
        class="pointer-events-none absolute inset-x-0 -top-16 h-32 bg-gradient-to-b opacity-70"
    />
    <!-- The icon centers against the text block: it is the only thing on its side, so the row's
         cross axis puts it halfway down the label / value / hint stack -->
    <div class="relative flex items-center justify-between">
      <div>
        <p class="text-xs uppercase tracking-[0.18em] text-muted">{{ label }}</p>
        <p class="mt-2 font-mono text-3xl font-semibold text-[#E6EDF7]">
          {{ display(props.value) }}
        </p>
        <p class="mt-1 text-xs text-muted/80">{{ hint }}</p>
      </div>
      <!-- Bare glyph at the plate's own 40px. Lucide scales its 24-unit artwork up to this size, which
           thickens the 2-unit default stroke on screen — so the stroke is dialled down to keep the
           drawn weight the same as an icon at its native size -->
      <component :is="icon" class="h-10 w-10 shrink-0 [stroke-width:1.5]"/>
    </div>
  </div>
</template>
