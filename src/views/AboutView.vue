<script lang="ts" setup>
import {computed, onMounted, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {Aperture, BadgeCheck, Box, Code, Lock, Monitor, PackageSearch, Scale, ShieldCheck} from 'lucide-vue-next'
import {type AppInfo, getAppInfo} from '../api/tauri'

const {t} = useI18n()

const info = ref<AppInfo | null>(null)

/** Tech stack entries: the framework/library name is universal and stays untranslated */
const stack = computed(() => [
  {icon: Monitor, name: 'Tauri', role: t('about.stack.desktop')},
  {icon: Code, name: 'Vue 3 · Tailwind CSS', role: t('about.stack.ui')},
  {icon: PackageSearch, name: 'maclarian', role: t('about.stack.parsing')},
  {icon: Box, name: 'three.js', role: t('about.stack.preview')},
])

onMounted(() => {
  getAppInfo()
      .then((result) => {
        info.value = result
      })
      .catch((err) => console.error('[app_info]', err))
})
</script>

<template>
  <!-- About is a plain, scrollable page: logo + version hero, intro, tech stack and credits -->
  <section class="flex min-h-0 w-full flex-col gap-4 overflow-y-auto px-8 pt-3 pb-8">
    <header>
      <h1 class="text-2xl font-semibold tracking-wide text-[#E6EDF7]">{{ $t('about.title') }}</h1>
      <p class="mt-1 text-sm text-muted">{{ $t('about.subtitle') }}</p>
    </header>

    <!-- Hero: app logo, name, tagline and version -->
    <div class="glass-card flex flex-wrap items-center gap-5 p-6">
      <div
          class="flex h-16 w-16 shrink-0 items-center justify-center rounded-2xl bg-gradient-to-br from-glow-cyan/25 to-glow-blue/10 text-glow-cyan ring-1 ring-cyan-300/30"
      >
        <Aperture class="h-8 w-8"/>
      </div>
      <div class="min-w-0 flex-1">
        <p class="text-lg font-semibold text-[#E6EDF7]">{{ $t('app.title') }}</p>
        <p class="mt-0.5 text-sm text-muted">{{ $t('app.subtitle') }}</p>
      </div>
      <span
          class="shrink-0 rounded-full border border-emerald-400/30 bg-emerald-400/10 px-3.5 py-1.5 font-mono text-xs font-medium text-emerald-300"
      >
        {{ $t('about.version') }} v{{ info?.version ?? '—' }}
      </span>
    </div>

    <div class="grid grid-cols-1 gap-4 xl:grid-cols-2">
      <!-- Introduction -->
      <div class="glass-card flex flex-col gap-4 p-6">
        <div class="flex items-center gap-2.5">
          <BadgeCheck class="h-4 w-4 text-glow-cyan"/>
          <p class="text-sm font-medium text-[#E6EDF7]">{{ $t('about.introTitle') }}</p>
        </div>
        <p class="text-sm leading-relaxed text-muted">{{ $t('about.intro') }}</p>
      </div>

      <!-- Tech stack -->
      <div class="glass-card p-6">
        <p class="text-xs uppercase tracking-[0.18em] text-muted">{{ $t('about.stackTitle') }}</p>
        <ul class="mt-4 flex flex-col gap-3">
          <li v-for="item in stack" :key="item.name" class="flex items-center gap-3">
            <div
                class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-white/5 text-glow-cyan ring-1 ring-cyan-300/15"
            >
              <component :is="item.icon" class="h-4 w-4"/>
            </div>
            <div class="min-w-0">
              <p class="truncate text-sm font-medium text-[#E6EDF7]">{{ item.name }}</p>
              <p class="text-xs text-muted/80">{{ item.role }}</p>
            </div>
          </li>
        </ul>
      </div>
    </div>

    <!-- Rights statement & privacy -->
    <div class="glass-card p-6">
      <div class="flex items-center gap-2.5">
        <ShieldCheck class="h-4 w-4 text-glow-cyan"/>
        <p class="text-sm font-medium text-[#E6EDF7]">{{ $t('about.rightsTitle') }}</p>
      </div>
      <p class="mt-3 text-sm leading-relaxed text-muted">{{ $t('about.disclaimer') }}</p>
      <div class="mt-5 border-t border-white/10 pt-5">
        <div class="flex items-center gap-2.5">
          <Lock class="h-4 w-4 text-glow-cyan"/>
          <p class="text-sm font-medium text-[#E6EDF7]">{{ $t('about.privacyTitle') }}</p>
        </div>
        <p class="mt-3 text-sm leading-relaxed text-muted">{{ $t('about.privacy') }}</p>
      </div>
      <div class="mt-5 border-t border-white/10 pt-5">
        <div class="flex items-center gap-2.5">
          <Scale class="h-4 w-4 text-glow-cyan"/>
          <p class="text-sm font-medium text-[#E6EDF7]">{{ $t('about.licenseTitle') }}</p>
        </div>
        <p class="mt-3 text-sm leading-relaxed text-muted">{{ $t('about.license') }}</p>
      </div>
    </div>
  </section>
</template>
