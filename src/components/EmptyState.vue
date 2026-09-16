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
      class="card flex flex-col items-center justify-center gap-3 px-6 py-14 text-center animate-fade-in"
  >
    <!-- Bare glyph with no slot behind it: the container gap is all that separates the icon from the
         title, so a bigger icon does not push the text further down -->
    <component :is="icon" class="h-10 w-10 text-accent"/>
    <h3 class="text-base font-semibold text-fg">{{ title }}</h3>
    <p v-if="description" class="max-w-md text-sm leading-relaxed text-muted">{{ description }}</p>
    <button v-if="actionLabel" class="btn-primary mt-2" type="button" @click="emit('action')">
      {{ actionLabel }}
    </button>
  </div>
</template>
