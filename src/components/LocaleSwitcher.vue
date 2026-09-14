<script lang="ts" setup>
import {computed, onBeforeUnmount, onMounted, ref} from 'vue'
import {Check, ChevronDown, Languages} from 'lucide-vue-next'
import {useI18n} from 'vue-i18n'
import {type LocaleCode, setLocale, SUPPORTED_LOCALES} from '../i18n'

const {locale, t} = useI18n()

const open = ref(false)
const root = ref<HTMLElement | null>(null)

const currentLabel = computed(
    () =>
        SUPPORTED_LOCALES.find((item) => item.code === String(locale.value))?.label ??
        String(locale.value),
)

function toggle() {
  open.value = !open.value
}

function select(code: LocaleCode) {
  setLocale(code)
  open.value = false
}

function onPointerDown(event: MouseEvent) {
  if (root.value && !root.value.contains(event.target as Node)) open.value = false
}

onMounted(() => document.addEventListener('mousedown', onPointerDown))
onBeforeUnmount(() => document.removeEventListener('mousedown', onPointerDown))
</script>

<template>
  <div ref="root" class="relative">
    <button
        :aria-expanded="open"
        :aria-label="t('language.label')"
        :title="t('language.label')"
        aria-haspopup="listbox"
        class="btn-ghost !px-3 !py-1.5 text-[11px]"
        type="button"
        @click="toggle"
    >
      <Languages class="h-3.5 w-3.5 shrink-0"/>
      <span class="font-mono tracking-wide">{{ currentLabel }}</span>
      <ChevronDown
          :class="open ? 'rotate-180' : ''"
          class="h-3.5 w-3.5 shrink-0 transition-transform duration-200"
      />
    </button>

    <Transition
        enter-active-class="transition duration-150 ease-out"
        enter-from-class="opacity-0 -translate-y-1"
        enter-to-class="opacity-100 translate-y-0"
        leave-active-class="transition duration-100 ease-in"
        leave-from-class="opacity-100"
        leave-to-class="opacity-0"
    >
      <ul
          v-if="open"
          class="absolute right-0 z-50 mt-2 w-44 overflow-hidden rounded-xl border border-edge/20 bg-ink-800/95 p-1 shadow-[var(--shadow-pop)] backdrop-blur-xl"
          role="listbox"
      >
        <li v-for="item in SUPPORTED_LOCALES" :key="item.code">
          <button
              :aria-selected="item.code === locale"
              :class="item.code === locale ? 'text-glow-cyan' : 'text-fg'"
              class="flex w-full items-center justify-between gap-2 rounded-lg px-3 py-2 text-left text-xs transition-colors hover:bg-tint-strong"
              role="option"
              type="button"
              @click="select(item.code)"
          >
            <span>{{ item.label }}</span>
            <Check v-if="item.code === locale" class="h-3.5 w-3.5 shrink-0"/>
          </button>
        </li>
      </ul>
    </Transition>
  </div>
</template>
