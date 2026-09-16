<script lang="ts" setup>
import {computed, defineAsyncComponent, ref} from 'vue'
import {ChevronDown, Download, FileArchive, FileBox, Grid2x2, Image as ImageIcon, MousePointerClick, Palette,} from 'lucide-vue-next'
import type {VisualAsset} from '../api/tauri'
import ExportDialog from './ExportDialog.vue'

/**
 * The preview is loaded on demand: it only mounts once an asset is selected, and it is itself the
 * component that pulls three.js in through `import()`, so deferring it keeps the renderer one step
 * further from the first paint.
 */
const ModelPreview = defineAsyncComponent(() => import('./ModelPreview.vue'))

const props = defineProps<{ asset: VisualAsset | null; loading: boolean }>()

const exportOpen = ref(false)

/**
 * One resource a material binds. Regular and virtual textures arrive in separate lists while a
 * material row shows them as one run of chips, so the kind is carried along: it picks the icon.
 */
interface MaterialBinding {
  kind: 'texture' | 'virtual'
  id: string
  name: string
  parameterName?: string | null
}

/**
 * Material → bindings. The payload only carries the relation one way round: every texture row
 * lists the material names that bind it, regular and virtual alike, so the material section has to
 * invert both. A name is the only key available (that is what the backend fills in), which also
 * means an unnamed material shows no bindings. Both lists are walked once per asset instead of
 * rescanning them for every material.
 */
const bindingsByMaterial = computed(() => {
  const byMaterial = new Map<string, MaterialBinding[]>()
  const bind = (materialNames: string[] | undefined, binding: MaterialBinding) => {
    for (const name of materialNames ?? []) {
      const bound = byMaterial.get(name)
      if (bound) {
        bound.push(binding)
      } else {
        byMaterial.set(name, [binding])
      }
    }
  }

  for (const texture of props.asset?.textures ?? []) {
    bind(texture.materialNames, {
      kind: 'texture',
      id: texture.id,
      name: texture.name || texture.id,
      parameterName: texture.parameterName,
    })
  }
  for (const virtualTexture of props.asset?.virtualTextures ?? []) {
    bind(virtualTexture.materialNames, {
      kind: 'virtual',
      id: virtualTexture.id,
      name: virtualTexture.name || virtualTexture.id,
      parameterName: virtualTexture.parameterName,
    })
  }
  return byMaterial
})

/** Material rows, each paired with the bindings it has (empty when none are named) */
const materialsWithBindings = computed(() =>
  (props.asset?.materials ?? []).map((material) => ({
    ...material,
    bindings: bindingsByMaterial.value.get(material.name) ?? [],
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
  <aside class="card flex min-h-0 flex-1 flex-col p-5">
    <div v-if="loading" class="flex flex-1 items-center justify-center py-16">
      <span class="text-sm text-muted">{{ $t('detail.loading') }}</span>
    </div>

    <div v-else-if="!asset" class="flex flex-1 flex-col items-center justify-center gap-3 py-16">
      <MousePointerClick class="h-6 w-6 text-muted"/>
      <p class="text-sm text-muted">{{ $t('detail.placeholder') }}</p>
    </div>

    <div v-else class="flex min-h-0 flex-1 flex-col gap-5 animate-fade-in">
      <div class="flex items-center justify-between gap-3">
        <p class="text-xs font-semibold text-muted">
          {{ $t('detail.visualAsset') }}
        </p>
        <button
            class="btn-ghost shrink-0 !h-7 !px-3 !text-xs"
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
              class="flex w-full items-center gap-2 text-xs font-semibold text-muted transition-colors duration-150 ease-fluent hover:text-fg"
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
              class="card-well mt-2">
            <p class="break-all font-mono text-[12px] text-accent">{{ asset.path }}</p>
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
              class="flex w-full items-center gap-2 text-xs font-semibold text-muted transition-colors duration-150 ease-fluent hover:text-fg disabled:cursor-default disabled:hover:text-muted"
              type="button"
              :aria-expanded="!collapsed.materials"
              :disabled="!asset.materials.length"
              @click="toggle('materials')"
          >
            <Palette class="h-3.5 w-3.5 shrink-0"/>
            <span>{{ $t('detail.materialLabel') }}</span>
            <span class="tabular-nums text-faint">{{ asset.materials.length }}</span>
            <ChevronDown
                v-if="asset.materials.length"
                class="ml-auto h-3.5 w-3.5 shrink-0 transition-transform duration-200"
                :class="collapsed.materials ? '-rotate-90' : ''"
            />
          </button>
          <div v-show="!collapsed.materials" class="mt-2 space-y-2">
            <div
                v-for="material in materialsWithBindings"
                :key="material.id"
                class="card-well"
            >
              <p v-if="material.sourceFile" class="break-all font-mono text-[12px] text-accent">
                {{ material.sourceFile }}
              </p>
              <!-- The GUID is the material's identity, not its name: names repeat across templates,
                   and the game's own XML refers to a material by this value. It sits next to the name
                   rather than in a `title`, because a native tooltip cannot be selected and the whole
                   point of showing it is to have it copied. Unnamed materials already read as their
                   GUID, so the two would be the same string twice. -->
              <p class="mt-1.5 flex flex-wrap items-baseline gap-x-2 font-mono text-[11px] text-muted">
                <span class="break-all">{{ material.name || material.id }}</span>
                <span v-if="material.name" class="break-all text-[10px] text-faint">
                  {{ material.id }}
                </span>
              </p>
              <!-- The archive belongs to the template path above, so it sits under the identity line
                   rather than after the bindings — the chips below are what the material binds -->
              <p
                  v-if="material.pak"
                  class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted"
              >
                <span
                    :title="$t('detail.pakLabel')"
                    class="flex min-w-0 items-center gap-1.5"
                >
                  <FileArchive class="h-3 w-3 shrink-0"/>
                  <span class="truncate font-mono">{{ material.pak }}</span>
                </span>
              </p>
              <ul
                  v-if="material.bindings.length"
                  class="mt-2.5 flex flex-wrap gap-1.5 border-t border-hairline pt-2"
              >
                <li
                    v-for="binding in material.bindings"
                    :key="binding.kind + binding.id"
                    class="flex max-w-full items-center gap-1.5 rounded-sm bg-tint px-1.5 py-1"
                >
                  <ImageIcon
                      v-if="binding.kind === 'texture'"
                      class="h-3 w-3 shrink-0 text-accent/70"
                  />
                  <Grid2x2 v-else class="h-3 w-3 shrink-0 text-accent/70"/>
                  <span class="break-all font-mono text-[11px] text-muted">{{ binding.name }}</span>
                  <span v-if="binding.parameterName" class="shrink-0 text-[10px] text-faint">
                    {{ binding.parameterName }}
                  </span>
                </li>
              </ul>
            </div>
            <p v-if="!asset.materials.length" class="text-xs text-subtle">
              {{ $t('detail.none') }}
            </p>
          </div>
        </div>

        <div class="space-y-5">
          <div>
            <button
                class="flex w-full items-center gap-2 text-xs font-semibold text-muted transition-colors duration-150 ease-fluent hover:text-fg disabled:cursor-default disabled:hover:text-muted"
                type="button"
                :aria-expanded="!collapsed.textures"
                :disabled="!asset.textures.length"
                @click="toggle('textures')"
            >
              <ImageIcon class="h-3.5 w-3.5 shrink-0"/>
              <span>{{ $t('detail.textureLabel') }}</span>
              <span class="tabular-nums text-faint">{{ asset.textures.length }}</span>
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
                  class="card-well"
              >
                <p class="break-all font-mono text-[12px] text-accent">{{ tex.path }}</p>
                <!-- The parameter stays out of the card: it describes a binding, not the resource,
                     and the material section — where a material is read as a whole — already shows
                     it. The card reports the name, the size and the archive, nothing else. -->
                <div class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted">
                  <span v-if="tex.name" class="break-all font-mono">{{ tex.name }}</span>
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
              <p v-if="!asset.textures.length" class="text-xs text-subtle">
                {{ $t('detail.noTextures') }}
              </p>
            </div>
          </div>

          <div>
            <button
                class="flex w-full items-center gap-2 text-xs font-semibold text-muted transition-colors duration-150 ease-fluent hover:text-fg disabled:cursor-default disabled:hover:text-muted"
                type="button"
                :aria-expanded="!collapsed.virtualTextures"
                :disabled="!asset.virtualTextures.length"
                @click="toggle('virtualTextures')"
            >
              <Grid2x2 class="h-3.5 w-3.5 shrink-0"/>
              <span>{{ $t('detail.virtualTextureLabel') }}</span>
              <span class="tabular-nums text-faint">{{ asset.virtualTextures.length }}</span>
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
                  class="card-well"
              >
                <p class="break-all font-mono text-[12px] text-accent">{{ vt.path || vt.hash }}</p>
                <!-- Same as the texture card: the parameter belongs to the material's binding, so
                     it is read in the material section (its chips) and not repeated here. -->
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
              <p v-if="!asset.virtualTextures.length" class="text-xs text-subtle">
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
