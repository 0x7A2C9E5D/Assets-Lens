<script lang="ts" setup>
import {computed, defineAsyncComponent, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ChevronDown, Download, FileArchive, FileBox, Grid2x2, Image as ImageIcon, MousePointerClick, Palette,} from 'lucide-vue-next'
import type {TextureRef, VisualAsset} from '../api/tauri'
import ExportDialog from './ExportDialog.vue'

/**
 * The preview is loaded on demand: it only mounts once an asset is selected, and it is itself the
 * component that pulls three.js in through `import()`, so deferring it keeps the renderer one step
 * further from the first paint.
 */
const ModelPreview = defineAsyncComponent(() => import('./ModelPreview.vue'))

const props = defineProps<{ asset: VisualAsset | null; loading: boolean }>()

useI18n()

const exportOpen = ref(false)

/**
 * Material → textures. The payload only carries the relation one way round: every texture row
 * lists the material names that bind it, so the material section has to invert it. A name is the
 * only key available (that is what the backend fills in), which also means an unnamed material
 * shows no textures. Built once per asset instead of rescanning the texture list for every row.
 */
const texturesByMaterial = computed(() => {
  const byMaterial = new Map<string, TextureRef[]>()
  for (const texture of props.asset?.textures ?? []) {
    for (const name of texture.materialNames ?? []) {
      const bound = byMaterial.get(name)
      if (bound) {
        bound.push(texture)
      } else {
        byMaterial.set(name, [texture])
      }
    }
  }
  return byMaterial
})

/** Material rows, each paired with the textures it binds (empty when none are named) */
const materialsWithTextures = computed(() =>
  (props.asset?.materials ?? []).map((material) => ({
    ...material,
    textures: texturesByMaterial.value.get(material.name) ?? [],
  })),
)

/**
 * Section collapsing: every section starts folded, so the panel opens as a compact summary — the
 * count in each title row is what tells the user whether a section is worth opening — and the rows
 * are pulled in on demand. A plain `ref` is enough: the state lives as long as the panel does (the
 * whole session) and deliberately survives switching assets, so a section the user opened stays
 * open. Content is toggled with `v-show`, not `v-if`: the rows are cheap to keep, and keeping them
 * avoids rebuilding dozens of cards on every expand.
 */
type SectionKey = 'mesh' | 'materials' | 'textures' | 'virtualTextures'

const collapsed = ref<Record<SectionKey, boolean>>({
  mesh: true,
  materials: true,
  textures: true,
  virtualTextures: true,
})

function toggle(section: SectionKey) {
  collapsed.value[section] = !collapsed.value[section]
}
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
      <div class="flex items-center justify-between gap-3">
        <p class="text-xs uppercase tracking-[0.18em] text-muted">
          {{ $t('detail.visualAsset') }}
        </p>
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
          <button
              class="flex w-full items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted transition-colors hover:text-[#E6EDF7]"
              type="button"
              :aria-expanded="!collapsed.mesh"
              @click="toggle('mesh')"
          >
            <FileBox class="h-3.5 w-3.5 shrink-0"/>
            <span>{{ $t('detail.mesh') }}</span>
            <ChevronDown
                class="ml-auto h-3.5 w-3.5 shrink-0 transition-transform duration-200"
                :class="collapsed.mesh ? '-rotate-90' : ''"
            />
          </button>
          <div
              v-show="!collapsed.mesh"
              class="mt-2 rounded-xl border border-white/5 bg-ink-900/50 p-3 transition-colors hover:border-cyan-300/25">
            <p class="break-all font-mono text-[12px] text-glow-cyan">{{ asset.path }}</p>
            <div class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted">
              <span class="break-all font-mono">{{ asset.name }}</span>
              <span
                  v-if="asset.meshPak"
                  :title="$t('detail.pakLabel')"
                  class="flex min-w-0 items-center gap-1.5"
              >
                <FileArchive class="h-3 w-3 shrink-0"/>
                <span class="truncate font-mono">{{ asset.meshPak }}</span>
              </span>
            </div>
          </div>
        </div>

        <div>
          <button
              class="flex w-full items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted transition-colors hover:text-[#E6EDF7] disabled:cursor-default disabled:hover:text-muted"
              type="button"
              :aria-expanded="!collapsed.materials"
              :disabled="!asset.materials.length"
              @click="toggle('materials')"
          >
            <Palette class="h-3.5 w-3.5 shrink-0"/>
            <span>{{ $t('detail.materialLabel') }}</span>
            <span class="tabular-nums text-muted/60">{{ asset.materials.length }}</span>
            <ChevronDown
                v-if="asset.materials.length"
                class="ml-auto h-3.5 w-3.5 shrink-0 transition-transform duration-200"
                :class="collapsed.materials ? '-rotate-90' : ''"
            />
          </button>
          <div v-show="!collapsed.materials" class="mt-2 space-y-2">
            <div
                v-for="material in materialsWithTextures"
                :key="material.id"
                :title="material.id"
                class="rounded-xl border border-white/5 bg-ink-900/50 p-3 transition-colors hover:border-cyan-300/25"
            >
              <p v-if="material.sourceFile" class="break-all font-mono text-[12px] text-glow-cyan">
                {{ material.sourceFile }}
              </p>
              <p class="mt-1.5 break-all font-mono text-[11px] text-muted">
                {{ material.name || material.id }}
              </p>
              <ul
                  v-if="material.textures.length"
                  class="mt-2.5 flex flex-wrap gap-1.5 border-t border-white/5 pt-2"
              >
                <li
                    v-for="texture in material.textures"
                    :key="texture.id"
                    class="flex max-w-full items-center gap-1.5 rounded-md bg-white/[0.04] px-1.5 py-1"
                >
                  <ImageIcon class="h-3 w-3 shrink-0 text-cyan-300/70"/>
                  <span class="break-all font-mono text-[11px] text-muted">
                    {{ texture.name || texture.id }}
                  </span>
                  <span v-if="texture.parameterName" class="shrink-0 text-[10px] text-muted/50">
                    {{ texture.parameterName }}
                  </span>
                </li>
              </ul>
            </div>
            <p v-if="!asset.materials.length" class="text-xs text-muted/70">
              {{ $t('detail.none') }}
            </p>
          </div>
        </div>

        <div class="space-y-5">
          <div>
            <button
                class="flex w-full items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted transition-colors hover:text-[#E6EDF7] disabled:cursor-default disabled:hover:text-muted"
                type="button"
                :aria-expanded="!collapsed.textures"
                :disabled="!asset.textures.length"
                @click="toggle('textures')"
            >
              <ImageIcon class="h-3.5 w-3.5 shrink-0"/>
              <span>{{ $t('detail.textureLabel') }}</span>
              <span class="tabular-nums text-muted/60">{{ asset.textures.length }}</span>
              <ChevronDown
                  v-if="asset.textures.length"
                  class="ml-auto h-3.5 w-3.5 shrink-0 transition-transform duration-200"
                  :class="collapsed.textures ? '-rotate-90' : ''"
              />
            </button>

            <div v-show="!collapsed.textures" class="mt-2 space-y-2">
              <div
                  v-for="tex in asset.textures"
                  :key="tex.id"
                  class="rounded-xl border border-white/5 bg-ink-900/50 p-3 transition-colors hover:border-cyan-300/25"
              >
                <p class="break-all font-mono text-[12px] text-glow-cyan">{{ tex.path }}</p>
                <!-- The name and the material slot are material-side facts: the material section already
                     lists them, so a texture row keeps only what belongs to the file itself. -->
                <div class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted">
                  <span>{{ tex.width }} × {{ tex.height }}</span>
                  <span
                      v-if="tex.source"
                      :title="$t('detail.pakLabel')"
                      class="flex min-w-0 items-center gap-1.5"
                  >
                    <FileArchive class="h-3 w-3 shrink-0"/>
                    <span class="truncate font-mono">{{ tex.source }}</span>
                  </span>
                </div>
              </div>
              <p v-if="!asset.textures.length" class="text-xs text-muted/70">
                {{ $t('detail.noTextures') }}
              </p>
            </div>
          </div>

          <div>
            <button
                class="flex w-full items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted transition-colors hover:text-[#E6EDF7] disabled:cursor-default disabled:hover:text-muted"
                type="button"
                :aria-expanded="!collapsed.virtualTextures"
                :disabled="!asset.virtualTextures.length"
                @click="toggle('virtualTextures')"
            >
              <Grid2x2 class="h-3.5 w-3.5 shrink-0"/>
              <span>{{ $t('detail.virtualTextureLabel') }}</span>
              <span class="tabular-nums text-muted/60">{{ asset.virtualTextures.length }}</span>
              <ChevronDown
                  v-if="asset.virtualTextures.length"
                  class="ml-auto h-3.5 w-3.5 shrink-0 transition-transform duration-200"
                  :class="collapsed.virtualTextures ? '-rotate-90' : ''"
              />
            </button>

            <div v-show="!collapsed.virtualTextures" class="mt-2 space-y-2">
              <div
                  v-for="vt in asset.virtualTextures"
                  :key="vt.id"
                  class="rounded-xl border border-white/5 bg-ink-900/50 p-3 transition-colors hover:border-cyan-300/25"
              >
                <p class="break-all font-mono text-[12px] text-glow-cyan">{{ vt.path || vt.hash }}</p>
                <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted">
                  <span class="break-all font-mono">{{ vt.name }}</span>
                  <span v-if="vt.width">{{ vt.width }} × {{ vt.height }}</span>
                  <span
                      v-if="vt.source"
                      :title="$t('detail.pakLabel')"
                      class="flex min-w-0 items-center gap-1.5"
                  >
                    <FileArchive class="h-3 w-3 shrink-0"/>
                    <span class="truncate font-mono">{{ vt.source }}</span>
                  </span>
                </div>
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
