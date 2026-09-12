<script lang="ts" setup>
import {defineAsyncComponent, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {
  Download,
  FileArchive,
  FileBox,
  Grid2x2,
  Image as ImageIcon,
  MousePointerClick,
  Palette,
} from 'lucide-vue-next'
import type {VisualAsset} from '../api/tauri'
import ExportDialog from './ExportDialog.vue'

/**
 * The preview is loaded on demand: it only mounts once an asset is selected, and it is itself the
 * component that pulls three.js in through `import()`, so deferring it keeps the renderer one step
 * further from the first paint.
 */
const ModelPreview = defineAsyncComponent(() => import('./ModelPreview.vue'))

defineProps<{ asset: VisualAsset | null; loading: boolean }>()

useI18n()

const exportOpen = ref(false)
</script>

<template>
  <aside class="glass-card flex min-h-0 flex-1 flex-col p-5">
    <div v-if="loading" class="flex flex-1 items-center justify-center py-16">
      <span class="text-sm text-muted">{{ $t('detail.loading') }}</span>
    </div>

    <div v-else-if="!asset" class="flex flex-1 flex-col items-center justify-center gap-3 py-16">
      <MousePointerClick class="h-6 w-6 text-muted"/>
      <p class="text-sm text-muted">{{ $t('detail.placeholder') }}</p>
    </div>

    <div v-else class="flex min-h-0 flex-1 flex-col gap-5 animate-fade-in">
      <div class="flex items-start justify-between gap-3">
        <div class="min-w-0">
          <p class="text-xs uppercase tracking-[0.18em] text-muted">
            {{ $t('detail.visualAsset') }}
          </p>
          <h3 class="mt-1 break-all font-mono text-base font-semibold text-[#E6EDF7]">
            {{ asset.name }}
          </h3>
        </div>
        <button
            class="btn-ghost shrink-0 !px-3 !py-1.5 !text-xs"
            type="button"
            @click="exportOpen = true"
        >
          <Download class="h-3.5 w-3.5"/>
          {{ $t('export.action') }}
        </button>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto space-y-5 pr-1">
        <ModelPreview :path="asset.path"/>

        <div>
          <p class="flex items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted">
            <FileBox class="h-3.5 w-3.5"/>
            {{ $t('detail.mesh') }}
          </p>
          <div class="mt-2 rounded-xl border border-white/5 bg-ink-900/50 p-3 transition-colors hover:border-cyan-300/25">
            <p class="break-all font-mono text-[12px] text-glow-cyan">{{ asset.path }}</p>
            <p
                v-if="asset.meshPak"
                :title="$t('detail.pakLabel')"
                class="mt-1.5 flex items-center gap-1.5 text-[11px] text-muted"
            >
              <FileArchive class="h-3 w-3 shrink-0"/>
              <span class="truncate font-mono">{{ asset.meshPak }}</span>
            </p>
          </div>
        </div>

        <div>
          <p class="flex items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted">
            <Palette class="h-3.5 w-3.5"/>
            {{ $t('detail.materialLabel') }}
          </p>
          <div class="mt-2 flex flex-wrap gap-1.5">
            <span
                v-for="id in asset.materialIds"
                :key="id"
                class="rounded-lg border border-cyan-300/15 bg-white/5 px-2 py-1 font-mono text-[11px] text-muted"
            >
              {{ id }}
            </span>
            <span v-if="!asset.materialIds.length" class="text-xs text-muted/70">
              {{ $t('detail.none') }}
            </span>
          </div>
        </div>

        <div class="space-y-5">
          <div>
            <p class="flex items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted">
              <ImageIcon class="h-3.5 w-3.5"/>
              {{ $t('detail.textureLabel') }}
            </p>

            <div class="mt-2 space-y-2">
              <div
                  v-for="tex in asset.textures"
                  :key="tex.id"
                  class="rounded-xl border border-white/5 bg-ink-900/50 p-3 transition-colors hover:border-cyan-300/25"
              >
                <p class="break-all font-mono text-[12px] text-glow-cyan">{{ tex.path }}</p>
                <div class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted">
                  <span>{{ tex.width }} × {{ tex.height }}</span>
                  <span v-if="tex.parameterName" class="text-glow-gold">{{ tex.parameterName }}</span>
                </div>
              </div>
              <p v-if="!asset.textures.length" class="text-xs text-muted/70">
                {{ $t('detail.noTextures') }}
              </p>
            </div>
          </div>

          <div>
            <p class="flex items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted">
              <Grid2x2 class="h-3.5 w-3.5"/>
              {{ $t('detail.virtualTextureLabel') }}
            </p>

            <div class="mt-2 space-y-2">
              <div
                  v-for="vt in asset.virtualTextures"
                  :key="vt.id"
                  class="rounded-xl border border-white/5 bg-ink-900/50 p-3 transition-colors hover:border-cyan-300/25"
              >
                <p class="break-all font-mono text-[12px] text-glow-cyan">{{ vt.name }}</p>
                <p class="mt-1 break-all font-mono text-[11px] text-muted">
                  <span class="text-muted/60">{{ $t('detail.hash') }}</span>
                  {{ vt.hash }}
                </p>
              </div>
              <p v-if="!asset.virtualTextures.length" class="text-xs text-muted/70">
                {{ $t('detail.noVirtualTextures') }}
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Export dialog: teleported to body so it is unaffected by the detail panel's scrolling and stacking context -->
    <ExportDialog v-if="exportOpen" :asset="asset" @close="exportOpen = false"/>
  </aside>
</template>
