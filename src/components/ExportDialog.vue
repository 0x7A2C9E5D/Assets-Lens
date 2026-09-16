<script lang="ts" setup>
import {computed, onBeforeUnmount, onMounted, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {Channel} from '@tauri-apps/api/core'
import {AlertTriangle, CheckCircle2, Download, FolderOpen, Loader2, RotateCcw, X,} from 'lucide-vue-next'
import {
  type ExportOptions,
  type ExportProgress,
  type ExportResult,
  exportVisualAsset,
  type ExportWarning,
  type MeshFormat,
  pickDirectory,
  type TextureFormat,
  type VisualAsset,
} from '../api/tauri'
import ProgressBar from './ProgressBar.vue'

/** The asset itself: the request is addressed by its GUID */
const props = defineProps<{ asset: VisualAsset | null }>()

const emit = defineEmits<{ close: [] }>()

const {t, te} = useI18n()

/** Mesh export format: raw GR2 or converted GLB */
const MESH_FORMATS: readonly MeshFormat[] = ['gr2', 'glb']

/** Texture export format: none / DDS / PNG */
const TEXTURE_FORMATS: readonly TextureFormat[] = ['none', 'dds', 'png']

const destDir = ref('')
const running = ref(false)
const errorMsg = ref('')
const progress = ref<ExportProgress | null>(null)
const result = ref<ExportResult | null>(null)
const meshFormat = ref<MeshFormat>('gr2')

/** Texture output format; asset.json is always generated */
const textureFormat = ref<TextureFormat>('dds')

const canStart = computed(() => !!props.asset && !!destDir.value && !running.value)

const phaseLabel = computed(() => {
  const phase = progress.value?.phase
  return phase ? t(`export.phase.${phase}`) : ''
})

async function chooseDir() {
  const picked = await pickDirectory(destDir.value || undefined)
  if (picked) destDir.value = picked
}

async function start() {
  if (!props.asset || !destDir.value) return

  running.value = true
  errorMsg.value = ''
  result.value = null
  progress.value = {phase: 'prepare', currentFile: null, percent: 0}

  const requested: ExportOptions = {
    meshFormat: meshFormat.value,
    textureFormat: textureFormat.value,
  }

  const channel = new Channel<ExportProgress>()
  channel.onmessage = (update) => {
    progress.value = update
  }

  try {
    result.value = await exportVisualAsset(
        props.asset.id,
        destDir.value,
        requested,
        channel,
    )
  } catch (err) {
    console.error('[export_visual_asset]', err)
    errorMsg.value = String(err)
  } finally {
    running.value = false
  }
}

function restart() {
  result.value = null
  progress.value = null
  errorMsg.value = ''
}

function requestClose() {
  if (running.value) return
  emit('close')
}

/** Warnings are localized by code; unknown codes (passed through from maclarian) fall back to the raw detail */
function warningText(warning: ExportWarning): string {
  const key = `export.warning.${warning.code}`
  return te(key) ? t(key, {detail: warning.detail}) : warning.detail
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') requestClose()
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <Teleport to="body">
    <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-veil p-4 animate-fade-in"
        @click.self="requestClose"
    >
      <!-- Fluent dialog: the card surface raised all the way to shadow64 -->
      <div
          class="card flex max-h-[86vh] w-[460px] max-w-full flex-col overflow-hidden shadow-[var(--shadow-dialog)] animate-fade-in"
          @click.stop
      >
        <!-- Title -->
        <header class="flex shrink-0 items-start justify-between gap-3 border-b border-hairline px-5 py-4">
          <div class="min-w-0">
            <h2 class="flex items-center gap-2 text-base font-semibold text-fg">
              <Download class="h-4 w-4 text-accent"/>
              {{ $t('export.title') }}
            </h2>
            <p
                :title="asset?.name"
                class="mt-1 truncate font-mono text-[12px] text-accent"
            >
              {{ asset ? asset.name : $t('export.emptyName') }}
            </p>
          </div>
          <button
              :aria-label="$t('export.close')"
              :disabled="running"
              class="rounded-md p-1.5 text-muted transition-colors duration-150 ease-fluent hover:bg-tint hover:text-fg disabled:opacity-40"
              type="button"
              @click="requestClose"
          >
            <X class="h-4 w-4"/>
          </button>
        </header>

        <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
          <!-- Destination directory -->
          <div class="card-well">
            <p class="text-xs font-semibold text-muted">
              {{ $t('export.destLabel') }}
            </p>
            <div class="mt-2 flex items-center gap-2">
              <p
                  :class="destDir ? 'text-fg' : 'text-faint'"
                  :title="destDir"
                  class="min-w-0 flex-1 truncate font-mono text-[12px]"
              >
                {{ destDir || $t('export.destEmpty') }}
              </p>
              <button :disabled="running" class="btn-ghost !h-7 !px-3 !text-xs" type="button" @click="chooseDir">
                <FolderOpen class="h-3.5 w-3.5"/>
                {{ $t('export.pickDir') }}
              </button>
            </div>
          </div>

          <!-- Mesh format -->
          <div v-if="!running && !result" class="space-y-2">
            <p class="text-xs font-semibold text-muted">
              {{ $t('export.meshFormatLabel') }}
            </p>
            <div class="grid grid-cols-2 gap-2">
              <button
                  v-for="fmt in MESH_FORMATS"
                  :key="fmt"
                  :aria-pressed="meshFormat === fmt"
                  :class="
                  meshFormat === fmt
                    ? 'border-accent bg-accent/10 shadow-[inset_0_0_0_1px_rgb(var(--c-accent))]'
                    : 'border-hairline-strong bg-ink-700 hover:bg-tint'
                "
                  class="rounded-md border px-3 py-2 text-left transition-colors duration-150 ease-fluent"
                  type="button"
                  @click="meshFormat = fmt"
              >
                <span class="block text-[13px] font-semibold text-fg">
                  {{ $t(`export.meshFormat.${fmt}`) }}
                </span>
                <span class="mt-0.5 block text-[11px] leading-relaxed text-muted">
                  {{ $t(`export.meshFormat.${fmt}Desc`) }}
                </span>
              </button>
            </div>
          </div>

          <!-- Texture format: three-way choice of none / DDS / PNG -->
          <div v-if="!running && !result" class="space-y-1.5">
            <p class="px-1 text-xs font-semibold text-muted">
              {{ $t('export.textureGroup') }}
            </p>
            <div class="grid grid-cols-3 gap-2">
              <button
                  v-for="fmt in TEXTURE_FORMATS"
                  :key="fmt"
                  :aria-pressed="textureFormat === fmt"
                  :class="
                  textureFormat === fmt
                    ? 'border-accent bg-accent/10 shadow-[inset_0_0_0_1px_rgb(var(--c-accent))]'
                    : 'border-hairline-strong bg-ink-700 hover:bg-tint'
                "
                  class="rounded-md border px-3 py-2 text-left transition-colors duration-150 ease-fluent"
                  type="button"
                  @click="textureFormat = fmt"
              >
                <span class="block text-[13px] font-semibold text-fg">
                  {{ $t(`export.textureFormat.${fmt}`) }}
                </span>
                <span class="mt-0.5 block text-[11px] leading-relaxed text-muted">
                  {{ $t(`export.textureFormat.${fmt}Desc`) }}
                </span>
              </button>
            </div>
          </div>

          <!-- Progress -->
          <div v-if="running" class="space-y-2 py-2">
            <ProgressBar :percent="progress?.percent ?? 0">
              <Loader2 class="h-3.5 w-3.5 shrink-0 animate-spin text-accent"/>
              {{ phaseLabel }}
            </ProgressBar>
            <p v-if="progress?.currentFile" class="truncate font-mono text-[11px] text-subtle">
              {{ progress.currentFile }}
            </p>
          </div>

          <!-- Result -->
          <div v-if="result" class="space-y-3 animate-fade-in">
            <div class="flex items-center gap-2 rounded-md border border-success/30 bg-success/10 px-3 py-2.5">
              <CheckCircle2 class="h-4 w-4 shrink-0 text-success"/>
              <span class="text-[13px] text-success">
                {{ $t('export.resultTitle', {count: result.files.length}) }}
              </span>
            </div>

            <div
                v-if="result.warnings.length"
                class="rounded-md border border-warning/30 bg-warning/10 px-3 py-2.5"
            >
              <p class="flex items-center gap-2 text-[12px] text-warning">
                <AlertTriangle class="h-3.5 w-3.5"/>
                {{ $t('export.warningsTitle', {count: result.warnings.length}) }}
              </p>
              <ul class="mt-1.5 space-y-1">
                <li
                    v-for="(warn, index) in result.warnings"
                    :key="`${warn.code}-${index}`"
                    class="break-all font-mono text-[11px] leading-relaxed text-muted"
                >
                  {{ warningText(warn) }}
                </li>
              </ul>
            </div>

            <div class="space-y-1.5">
              <p class="text-xs font-semibold text-muted">
                {{ $t('export.filesTitle', {count: result.files.length}) }}
              </p>
              <div
                  v-for="file in result.files"
                  :key="file.path"
                  class="flex items-center gap-2 rounded-sm bg-ink-800 px-3 py-2"
              >
                <span class="min-w-0 flex-1 truncate font-mono text-[11px] text-fg">
                  {{ baseName(file.path) }}
                </span>
                <span class="shrink-0 font-mono text-[11px] text-accent">
                  {{ formatSize(file.sizeBytes) }}
                </span>
              </div>
              <p :title="result.outputDir" class="break-all pt-1 font-mono text-[11px] text-subtle">
                {{ result.outputDir }}
              </p>
            </div>
          </div>

          <!-- Error -->
          <div
              v-if="errorMsg"
              class="flex items-start gap-2 rounded-md border border-danger/30 bg-danger/10 px-3 py-2.5"
          >
            <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0 text-danger"/>
            <span class="break-all text-[12px] leading-relaxed text-danger">{{ errorMsg }}</span>
          </div>
        </div>

        <!-- Actions -->
        <footer class="flex shrink-0 items-center justify-end gap-2 border-t border-hairline px-5 py-3">
          <button :disabled="running" class="btn-ghost !h-7 !text-xs" type="button" @click="requestClose">
            {{ $t('export.close') }}
          </button>
          <button
              v-if="result"
              class="btn-ghost !h-7 !text-xs"
              type="button"
              @click="restart"
          >
            <RotateCcw class="h-3.5 w-3.5"/>
            {{ $t('export.again') }}
          </button>
          <button
              v-if="!result"
              :disabled="!canStart"
              class="btn-primary !h-7 !text-xs"
              type="button"
              @click="start"
          >
            <Loader2 v-if="running" class="h-3.5 w-3.5 animate-spin"/>
            <Download v-else class="h-3.5 w-3.5"/>
            {{ running ? $t('export.running') : $t('export.start') }}
          </button>
        </footer>
      </div>
    </div>
  </Teleport>
</template>
