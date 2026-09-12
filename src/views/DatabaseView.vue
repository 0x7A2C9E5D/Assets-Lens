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
  Package,
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
const progress = ref<BuildProgress>({current: 0, total: 0, currentFile: null, percent: 0})

const statusText = computed(() => {
  if (building.value) return t('database.status.building')
  if (stats.value) return t('database.status.ready')
  if (gamePath.value) return t('database.status.pending')
  return t('database.status.unset')
})

const statusClass = computed(() => {
  if (building.value) return 'border-glow-gold/40 text-glow-gold'
  if (stats.value) return 'border-emerald-400/40 text-emerald-300'
  if (gamePath.value) return 'border-glow-cyan/40 text-glow-cyan'
  return 'border-white/10 text-muted'
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
  progress.value = {current: 0, total: 0, currentFile: null, percent: 0}

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
        <h1 class="text-2xl font-semibold tracking-wide text-[#E6EDF7]">
          {{ $t('database.title') }}
        </h1>
        <p class="mt-1 text-sm text-muted">{{ $t('database.subtitle') }}</p>
      </div>
      <span
          :class="statusClass"
          class="flex items-center gap-2 self-end rounded-full border bg-white/5 px-3 py-1.5 text-xs"
      >
        <Loader2 v-if="building" class="h-3.5 w-3.5 animate-spin"/>
        {{ statusText }}
      </span>
    </header>

    <div
        v-if="errorMsg"
        class="flex items-start gap-3 rounded-2xl border border-red-400/30 bg-red-500/10 px-4 py-3 text-sm text-red-200"
    >
      <TriangleAlert class="mt-0.5 h-4 w-4 shrink-0"/>
      <span class="break-all">{{ errorMsg }}</span>
    </div>

    <div class="glass-card p-6">
      <div class="flex items-center justify-between gap-4">
        <div class="min-w-0">
          <p class="text-xs uppercase tracking-[0.18em] text-muted">
            {{ $t('database.dir.label') }}
          </p>
          <p class="mt-2 break-all font-mono text-sm text-[#E6EDF7]">
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
      <p class="mt-3 text-xs leading-relaxed text-muted/80">{{ $t('database.dir.tip') }}</p>
    </div>

    <div class="glass-card p-6">
      <div class="flex flex-wrap items-center justify-between gap-4">
        <div>
          <p class="text-xs uppercase tracking-[0.18em] text-muted">
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

    <!-- Five across only once the window can actually hold them: with the 300px sidebar a
         1280-wide window leaves ~170px per card, which is not enough for a five-digit count -->
    <div class="grid grid-cols-2 gap-4 md:grid-cols-3 2xl:grid-cols-5">
      <StatCard
          :hint="$t('stats.visualHint')"
          :icon="Boxes"
          :value="stats ? stats.visualCount : '—'"
          accent="cyan"
          label="Visual"
      />
      <StatCard
          :hint="$t('stats.materialHint')"
          :icon="Palette"
          :value="stats ? stats.materialCount : '—'"
          accent="blue"
          label="Material"
      />
      <StatCard
          :hint="$t('stats.textureHint')"
          :icon="ImageIcon"
          :value="stats ? stats.textureCount : '—'"
          accent="gold"
          label="Texture"
      />
      <StatCard
          :hint="$t('stats.virtualHint')"
          :icon="Grid2x2"
          :value="stats ? stats.virtualTextureCount : '—'"
          accent="green"
          label="Virtual Textures"
      />
      <!-- The accent reuses the app's own cyan rather than introducing a fifth color: it sits far
           enough from the neighbouring "Virtual Textures" card to not read as a duplicate -->
      <StatCard
          :hint="$t('stats.modHint')"
          :icon="Package"
          :value="stats ? stats.modCount : '—'"
          accent="cyan"
          label="Mods"
      />
    </div>
  </section>
</template>
