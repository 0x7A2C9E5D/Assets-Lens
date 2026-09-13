<script lang="ts" setup>
import {computed} from 'vue'
import {useI18n} from 'vue-i18n'
import {
  Aperture,
  ArrowUpRight,
  BadgeCheck,
  Box,
  Code,
  LoaderCircle,
  Lock,
  Monitor,
  PackageSearch,
  Scale,
  ShieldCheck,
} from 'lucide-vue-next'
import {openExternal} from '../api/tauri'
import {useReleaseCheck} from '../utils/release'

const {t} = useI18n()

/**
 * Version state is app-wide: both versions are resolved once per launch (see `utils/release`), so
 * opening this page only reads the outcome instead of querying again. `remoteVersion` is only ever
 * shown as the tooltip of the update arrow, never as text on the page.
 */
const {localVersion, remoteVersion, releaseState} = useReleaseCheck()

/**
 * Credits rows: every entry links to the project it thanks (the Vue row credits two separate
 * projects, so its two names are independent links), while the role text stays untranslated-free
 */
const stack = computed(() => [
  {
    icon: Monitor,
    parts: [{label: 'Tauri', href: 'https://tauri.app/'}],
    role: t('about.stack.desktop'),
  },
  {
    icon: Code,
    parts: [
      {label: 'Vue.js', href: 'https://vuejs.org/'},
      {label: 'Tailwind CSS', href: 'https://tailwindcss.com/'},
    ],
    role: t('about.stack.ui'),
  },
  {
    icon: PackageSearch,
    parts: [{label: 'maclarian', href: 'https://crates.io/crates/maclarian'}],
    role: t('about.stack.parsing'),
  },
  {
    icon: Box,
    parts: [{label: 'three.js', href: 'https://threejs.org/'}],
    role: t('about.stack.preview'),
  },
])

/** Nexus Mods mark (Simple Icons, 24x24 viewBox) — no Lucide equivalent exists for this brand */
const NEXUS_MODS_PATH =
    'M17.376 0c-.993 0-2.18.686-2.907 1.182-1.676-.36-4.036-.545-6.787.635-1.365-.513-2.425-.562-3.32-.488a2.16 2.16 0 0 0-1.27.429c-.33.22-2.788 2.69-3.069 4.652C-.15 7.508.68 8.932 1.218 9.718c-.44 1.76-.2 4.572.517 6.188-.353 1.041-.713 2.089-.664 3.205.01.584.061 1.188.398 1.684C1.72 21.19 4.528 24 6.545 24c.957 0 1.93-.428 3.07-1.24 2.16.383 4.402.348 6.448-.532 2.573 1.001 4.224.625 4.84.162.587-.457 2.826-2.915 3.07-4.622.1-.672-.023-1.638-1.226-3.397a10.983 10.983 0 0 0-.501-6.455c.396-1.069.673-2.188.59-3.337-.015-.68-.221-1.167-.487-1.507-.209-.335-2.415-2.39-4.028-2.91A3.105 3.105 0 0 0 17.376 0m-.03 2.082c.65.015 2.155 1.093 3.01 1.906l.355.34c-.959-.163-2.125.428-3.26 1.55a10.28 10.28 0 0 0-1.358 1.595c-.28.384-.517.768-.753 1.285l1.18.635-3.895 1.477-1.122-4.18 1.033.547c1.358-3.102 2.524-3.973 3.232-4.416h.015a5.12 5.12 0 0 1 1.49-.724zM12 3.065a8.932 8.932 0 0 1 2.22.279 7.67 7.67 0 0 0-.42.488 8.403 8.403 0 0 0-1.8-.196 8.336 8.336 0 0 0-5.897 2.432 7.86 7.86 0 0 1-.37-.433A8.905 8.905 0 0 1 12 3.065m-7.076.305c.71-.002 1.309.127 2.2.466a9.526 9.526 0 0 0-1.713 1.337c-.327-.542-.624-1.156-.488-1.803m-.606.042c-.162.96.428 2.126 1.55 3.264.457.487 1.003.945 1.594 1.358.383.281.767.517 1.283.754l.62-1.182 1.49 3.914-4.176 1.122.546-1.033c-3.099-1.36-3.969-2.526-4.412-3.235v-.015a5.144 5.144 0 0 1-.723-1.491l-.015-.074c.015-.65 1.092-2.156 1.904-3.013Zm16.035 1.483a1.259 1.259 0 0 1 .26.015l.14.023a5.05 5.05 0 0 1-.13 1.137v.015c-.1.383-.228.765-.377 1.148a9.526 9.526 0 0 0-1.346-1.776c.547-.357 1.051-.546 1.453-.562M18.43 5.8a8.903 8.903 0 0 1 2.506 6.2 8.937 8.937 0 0 1-.27 2.183 7.658 7.658 0 0 0-.488-.425A8.407 8.407 0 0 0 20.364 12 8.334 8.334 0 0 0 18 6.173a7.904 7.904 0 0 1 .429-.373M3.315 9.905c.157.148.319.29.488.425A8.417 8.417 0 0 0 3.636 12c0 2.248.887 4.286 2.327 5.788a8.11 8.11 0 0 1-.426.376A8.902 8.902 0 0 1 3.065 12a8.937 8.937 0 0 1 .25-2.095m13.988 1.541-.546 1.034c3.098 1.359 3.969 2.526 4.412 3.235v.014c.34.488.575.99.723 1.492l.014.074c-.014.65-1.092 2.156-1.903 3.013l-.34.354c.163-.96-.427-2.127-1.549-3.264a10.298 10.298 0 0 0-1.594-1.359 7.008 7.008 0 0 0-1.283-.753l-.605 1.152-1.505-3.87zm-6.006 1.684 1.121 4.18-1.033-.547c-1.357 3.102-2.523 3.973-3.231 4.416h-.015c-.487.34-.989.576-1.49.724l-.074.015c-.65-.015-2.154-1.093-3.01-1.906l-.354-.34c.959.163 2.124-.428 3.26-1.55.488-.458.945-1.004 1.358-1.595.28-.384.517-.768.753-1.285l-1.166-.635ZM3.72 16.663A9.526 9.526 0 0 0 5.086 18.5c-.697.47-1.33.665-1.777.59l-.138-.024c0-.367.038-.748.128-1.137v-.015c.11-.417.254-.835.42-1.252m14.131 1.314c.129.14.253.283.372.43A8.904 8.904 0 0 1 12 20.936a8.932 8.932 0 0 1-2.282-.296 7.757 7.757 0 0 0 .417-.487 8.335 8.335 0 0 0 7.716-2.175m.696.889c.43.666.607 1.267.534 1.698l-.023.138a5.034 5.034 0 0 1-1.136-.128h-.014a10.718 10.718 0 0 1-1.114-.366 9.526 9.526 0 0 0 1.753-1.342'

/** GitHub mark (Simple Icons, 24x24 viewBox) */
const GITHUB_PATH =
    'M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12'

const GITHUB_URL = 'https://github.com/0x7A2C9E5D/Assets-Lens'
const NEXUS_MODS_URL = 'https://www.nexusmods.com/baldursgate3/mods/24924'

/** External project links: the brand marks are inline SVG paths so the logos stay recognizable */
const links = computed(() => [
  {
    name: t('about.nexusmods'),
    href: NEXUS_MODS_URL,
    path: NEXUS_MODS_PATH,
  },
  {
    name: t('about.github'),
    href: GITHUB_URL,
    path: GITHUB_PATH,
  },
])

function openLink(url: string) {
  openExternal(url).catch((err) => console.error('[open_external]', err))
}

</script>

<template>
  <!-- About is a plain, scrollable page: identity hero, intro, tech stack and credits -->
  <section class="flex min-h-0 w-full flex-col gap-4 overflow-y-auto px-8 pt-3 pb-8">
    <header>
      <h1 class="text-2xl font-semibold tracking-wide text-[#E6EDF7]">{{ $t('about.title') }}</h1>
      <p class="mt-1 text-sm text-muted">{{ $t('about.subtitle') }}</p>
    </header>

    <!-- App header + project links: a single card so the hero identity and the external resources share one panel -->
    <div class="glass-card p-6">
      <div class="flex flex-wrap items-center gap-3">
        <!-- The mark stands on its own: the glyph is the whole element, so the gap to the title is just
             the row gap — no slot or plate padding sits between them -->
        <Aperture class="h-8 w-8 shrink-0 text-glow-cyan"/>
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-2">
            <p class="text-lg font-semibold text-[#E6EDF7]">{{ $t('app.title') }}</p>
            <!-- Version pill: the running version plus, at most, one mark that the check produced.
                 The arrow lives inside the pill so the row reads as a single unit, and its tooltip is
                 the only place the published version number ever appears. "Up to date" and a failed
                 check add nothing, so the arrow is never shown for less than a newer release -->
            <span
                :class="[
                  'inline-flex items-center gap-1 rounded-md border border-emerald-400/25 bg-emerald-400/10 py-0.5 pl-1.5 font-mono text-[10px] font-medium leading-none text-emerald-300',
                  releaseState === 'outdated' ? 'pr-0.5' : 'pr-1.5',
                ]"
            >
              v{{ localVersion || '—' }}
              <button
                  v-if="releaseState === 'outdated'"
                  :aria-label="$t('about.updateAvailable', {version: remoteVersion})"
                  :title="$t('about.updateAvailable', {version: remoteVersion})"
                  class="flex h-3.5 w-3.5 items-center justify-center rounded-[3px] text-amber-300 transition-colors duration-200 hover:bg-amber-400/20 hover:text-amber-200"
                  type="button"
                  @click="openLink(NEXUS_MODS_URL)"
              >
                <ArrowUpRight class="h-3 w-3"/>
              </button>
              <LoaderCircle
                  v-else-if="releaseState === 'checking'"
                  :title="$t('about.checking')"
                  class="h-3 w-3 animate-spin text-muted"
              />
            </span>
          </div>
          <p class="mt-0.5 text-sm text-muted">{{ $t('app.subtitle') }}</p>
        </div>
        <!-- External links: opened in the system browser through the Tauri opener plugin -->
        <div class="flex shrink-0 items-center gap-2">
          <button
              v-for="link in links"
              :key="link.href"
              :aria-label="link.name"
              :title="link.name"
              class="flex h-8 w-8 items-center justify-center rounded-lg bg-white/5 text-glow-cyan ring-1 ring-cyan-300/15 transition-all duration-200 hover:bg-white/10 hover:ring-cyan-300/35"
              type="button"
              @click="openLink(link.href)"
          >
            <svg aria-hidden="true" class="h-4 w-4" fill="currentColor" viewBox="0 0 24 24">
              <path :d="link.path"/>
            </svg>
          </button>
        </div>
      </div>
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
          <li v-for="item in stack" :key="item.role" class="flex items-center gap-2">
            <!-- Bare glyphs, matching the hero mark. Every icon is the same size, so the row supplies the
                 spacing directly — no slot sits between a glyph and its label -->
            <component :is="item.icon" class="h-6 w-6 shrink-0 text-glow-cyan"/>
            <div class="min-w-0">
              <p class="flex flex-wrap items-center gap-x-1.5 text-sm font-medium text-[#E6EDF7]">
                <template v-for="(part, index) in item.parts" :key="part.href">
                  <span v-if="index" class="text-muted/40">·</span>
                  <!-- Two projects share one row, so each name is its own link. The arrow is always
                       rendered (dimmed) instead of appearing on hover: reserving its width keeps the
                       text from shifting when the pointer arrives -->
                  <button
                      :aria-label="part.href"
                      :title="part.href"
                      class="group/link inline-flex items-center gap-1 text-left transition-colors duration-200 hover:text-glow-cyan"
                      type="button"
                      @click="openLink(part.href)"
                  >
                    {{ part.label }}
                    <ArrowUpRight
                        class="h-3 w-3 text-muted/40 transition-colors duration-200 group-hover/link:text-glow-cyan"/>
                  </button>
                </template>
              </p>
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
