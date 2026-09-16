<script lang="ts" setup>
import {computed, onMounted, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {Channel} from '@tauri-apps/api/core'
import {
  Boxes,
  FolderOpen,
  Grid2x2,
  Image as ImageIcon,
  Loader2,
  Palette,
  Radar,
  RefreshCw,
  TriangleAlert,
} from 'lucide-vue-next'
import ProgressBar from '../components/ProgressBar.vue'
import StatCard from '../components/StatCard.vue'
import {
  buildDatabase,
  type BuildProgress,
  type DatabaseStats,
  dbStats,
  detectGamePath,
  getGamePath,
  pickDirectory,
  setGamePath,
} from '../api/tauri'
import {clearGamePath, readGamePath, writeGamePath} from '../utils/settings'

const {t} = useI18n()

const gamePath = ref<string | null>(null)
const detecting = ref(false)
const building = ref(false)
const stats = ref<DatabaseStats | null>(null)
const errorMsg = ref('')
const elapsed = ref(0)
const progress = ref<BuildProgress>({percent: 0})

/** The status chip is a Fluent Badge: an outline in the state's own colour over a neutral fill */
const status = computed(() => {
  if (building.value) return {text: t('database.status.building'), class: 'border-warning-line text-warning'}
  if (stats.value) return {text: t('database.status.ready'), class: 'border-success-line text-success'}
  if (gamePath.value) return {text: t('database.status.pending'), class: 'border-accent text-accent'}
  return {text: t('database.status.unset'), class: 'border-hairline-strong text-muted'}
})

/**
 * Compare two directories: the backend returns `PathBuf::display()` output while localStorage
 * keeps the raw user input, so the two may only differ in separators or casing. Normalize before
 * comparing, otherwise an unchanged directory is mistaken for a new one.
 */
function isSamePath(a: string | null, b: string | null): boolean {
  if (!a || !b) return false
  const normalize = (value: string) => value.replace(/[\\/]+$/, '').replace(/\\/g, '/').toLowerCase()
  return normalize(a) === normalize(b)
}

/**
 * Hand a directory to the backend (it validates Shared.pak and rebuilds the resolver) and remember
 * it in localStorage on success. Returns whether it took effect; a failure clears the stale record.
 */
async function applyGamePath(path: string): Promise<boolean> {
  try {
    const confirmed = await setGamePath(path)
    gamePath.value = confirmed
    // The directory changed, so any built database no longer matches it
    stats.value = null
    writeGamePath(confirmed)
    return true
  } catch (err) {
    console.error('[set_game_path]', err)
    clearGamePath()
    return false
  }
}

/**
 * Ask the backend for an auto-detected directory. Nothing is written to the UI before it answers:
 * the busy flag is only raised for a manual click, and the result is applied only when it differs
 * from what the page already shows — a silent probe on startup therefore never repaints or clears
 * anything of its own accord.
 */
function runDetect(manual = false) {
  if (manual) detecting.value = true

  detectGamePath()
      .then((path) => {
        if (!path) {
          // Auto-detection failed (no game at the default locations): keep the existing path and only
          // show a hint, so a manually chosen directory is never wiped out
          errorMsg.value = t('database.detectFailed')
          return
        }
        errorMsg.value = ''
        if (isSamePath(gamePath.value, path)) return
        gamePath.value = path
        stats.value = null
        // Auto-detected directories are remembered too, so the next launch reuses them
        writeGamePath(path)
      })
      .catch((err) => {
        console.error('[detect_game_path]', err)
        errorMsg.value = t('errors.general')
      })
      .finally(() => {
        detecting.value = false
      })
}

function chooseDirectory() {
  errorMsg.value = ''
  pickDirectory(gamePath.value ?? undefined)
      .then((dir) => (dir ? applyGamePath(dir) : true))
      .catch((err) => {
        console.error('[set_game_path]', err)
        return false
      })
      .then((ok) => {
        if (!ok) errorMsg.value = t('errors.directoryFailed')
      })
}

function build() {
  if (!gamePath.value) return
  building.value = true
  errorMsg.value = ''
  progress.value = {percent: 0}

  const started = Date.now()

  const channel = new Channel<BuildProgress>()
  channel.onmessage = (payload) => {
    progress.value = payload
    // Progress events arrive continuously, so the elapsed time rides along with them
    elapsed.value = Date.now() - started
  }

  buildDatabase(channel)
      .then((result) => {
        stats.value = result
        elapsed.value = Date.now() - started
      })
      .catch((err) => {
        console.error('[build_database]', err)
        errorMsg.value = t('errors.buildFailed')
      })
      .finally(() => {
        building.value = false
      })
}

onMounted(() => {
  // 1) Restore the database build state
  dbStats()
      .then((result) => {
        stats.value = result
      })
      .catch((err) => console.error('[db_stats]', err))

  // 2) Restore the game directory: the localStorage record wins (remembered across sessions); when
  //    the backend already uses the same directory just reuse it instead of calling set again
  //    (set clears the built database, so doing it on every page visit would force a rescan).
  //    Auto-detection only runs when neither is available, so a manually chosen directory is safe
  const stored = readGamePath()
  getGamePath()
      .then((current) => {
        if (isSamePath(current, stored)) {
          gamePath.value = current
          return true
        }
        return stored ? applyGamePath(stored) : false
      })
      .catch((err) => {
        console.error('[get_game_path]', err)
        return stored ? applyGamePath(stored) : false
      })
      .then((restored) => {
        if (!restored) runDetect()
      })
})
</script>

<template>
  <!-- Full-page responsive layout: the section stretches with the main area; no central canvas or outer gutters -->
  <section class="flex min-h-0 w-full flex-1 flex-col gap-4 px-8 pt-3 pb-8">
    <header class="flex flex-wrap items-start justify-between gap-4">
      <div>
        <h1 class="font-display text-2xl font-semibold text-fg">
          {{ $t('database.title') }}
        </h1>
        <p class="mt-1 text-sm text-muted">{{ $t('database.subtitle') }}</p>
      </div>
      <span
          :class="status.class"
          class="flex items-center gap-2 self-end rounded-full border bg-tint px-3 py-1 text-xs font-semibold"
      >
        <Loader2 v-if="building" class="h-3.5 w-3.5 animate-spin"/>
        {{ status.text }}
      </span>
    </header>

    <div
        v-if="errorMsg"
        class="flex items-start gap-3 rounded-md border border-danger/30 bg-danger/10 px-4 py-3 text-sm text-danger"
    >
      <TriangleAlert class="mt-0.5 h-4 w-4 shrink-0"/>
      <span class="break-all">{{ errorMsg }}</span>
    </div>

    <div class="card p-6">
      <div class="flex items-center justify-between gap-4">
        <div class="min-w-0">
          <p class="text-xs font-semibold text-muted">
            {{ $t('database.dir.label') }}
          </p>
          <p class="mt-2 break-all font-mono text-sm text-fg">
            {{ gamePath ?? $t('database.dir.empty') }}
          </p>
        </div>
        <div class="flex shrink-0 gap-2">
          <button :disabled="detecting" class="btn-ghost" type="button" @click="runDetect(true)">
            <Radar :class="detecting ? 'animate-pulse' : ''" class="h-4 w-4"/>
            {{ $t('database.dir.detect') }}
          </button>
          <button class="btn-ghost" type="button" @click="chooseDirectory">
            <FolderOpen class="h-4 w-4"/>
            {{ $t('database.dir.choose') }}
          </button>
        </div>
      </div>
      <p class="mt-3 text-xs leading-relaxed text-subtle">{{ $t('database.dir.tip') }}</p>
    </div>

    <div class="card p-6">
      <div class="flex flex-wrap items-center justify-between gap-4">
        <div>
          <p class="text-xs font-semibold text-muted">
            {{ stats ? $t('database.build.rebuildTitle') : $t('database.build.title') }}
          </p>
          <p class="mt-2 text-sm text-muted">
            {{ stats ? $t('database.build.rebuildDesc') : $t('database.build.desc') }}
          </p>
        </div>
        <div class="flex items-center gap-3">
          <span v-if="elapsed" class="font-mono text-xs text-muted">
            {{ $t('database.build.elapsed', {s: (elapsed / 1000).toFixed(1)}) }}
          </span>
          <button :disabled="!gamePath || building" class="btn-primary" type="button" @click="build">
            <RefreshCw :class="building ? 'animate-spin' : ''" class="h-4 w-4"/>
            {{
              building
                  ? $t('database.build.actionBuilding')
                  : stats
                      ? $t('database.build.actionRebuild')
                      : $t('database.build.actionBuild')
            }}
          </button>
        </div>
      </div>

      <div v-if="building" class="mt-5">
        <ProgressBar :percent="progress.percent"/>
      </div>
    </div>

    <!-- Four across only once the window can actually hold them: with the 300px sidebar a
         1280-wide window leaves ~210px per card, which is not enough for a five-digit count -->
    <div class="grid grid-cols-2 gap-4 md:grid-cols-4">
      <StatCard
          :hint="$t('stats.visualHint')"
          :icon="Boxes"
          :value="stats ? stats.visualCount : '—'"
          accent="brand"
          label="Visuals"
      />
      <StatCard
          :hint="$t('stats.materialHint')"
          :icon="Palette"
          :value="stats ? stats.materialCount : '—'"
          accent="success"
          label="Materials"
      />
      <StatCard
          :hint="$t('stats.textureHint')"
          :icon="ImageIcon"
          :value="stats ? stats.textureCount : '—'"
          accent="warning"
          label="Textures"
      />
      <StatCard
          :hint="$t('stats.virtualHint')"
          :icon="Grid2x2"
          :value="stats ? stats.virtualTextureCount : '—'"
          accent="neutral"
          label="Virtual Textures"
      />
    </div>
  </section>
</template>
