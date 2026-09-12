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
    <div class="relative flex items-start justify-between">
      <div>
        <p class="text-xs uppercase tracking-[0.18em] text-muted">{{ label }}</p>
        <p class="mt-2 font-mono text-3xl font-semibold text-[#E6EDF7]">
          {{ display(props.value) }}
        </p>
        <p class="mt-1 text-xs text-muted/80">{{ hint }}</p>
      </div>
      <span
          class="flex h-10 w-10 items-center justify-center rounded-xl border border-white/10 bg-white/5"
      >
        <component :is="icon" class="h-5 w-5"/>
      </span>
    </div>
  </div>
</template>
