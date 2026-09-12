<script lang="ts" setup>
defineProps<{
  icon: unknown
  title: string
  description?: string
  actionLabel?: string
}>()

const emit = defineEmits<{ action: [] }>()
</script>

<template>
  <div
      class="glass-card flex flex-col items-center justify-center gap-3 px-6 py-14 text-center animate-fade-in"
  >
    <span
        class="flex h-14 w-14 items-center justify-center rounded-2xl border border-cyan-300/20 bg-white/5 text-glow-cyan"
    >
      <component :is="icon" class="h-6 w-6"/>
    </span>
    <h3 class="text-base font-semibold text-[#E6EDF7]">{{ title }}</h3>
    <p v-if="description" class="max-w-md text-sm leading-relaxed text-muted">{{ description }}</p>
    <button v-if="actionLabel" class="btn-primary mt-2" type="button" @click="emit('action')">
      {{ actionLabel }}
    </button>
  </div>
</template>
