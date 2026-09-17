<script lang="ts" setup>
import {computed, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {Box, Loader2} from 'lucide-vue-next'
// Type-only import: emits no runtime code. three itself is lazy-loaded below through dynamic
// imports so the ~630 KB renderer never lands in the initial bundle.
import type * as THREE from 'three'
import {getVisualPreview, type ModelPreview} from '../api/tauri'
import {theme} from '../utils/theme'

type ThreeModule = typeof THREE
type GltfCtor = typeof import('three/examples/jsm/loaders/GLTFLoader.js').GLTFLoader
type OrbitCtor = typeof import('three/examples/jsm/controls/OrbitControls.js').OrbitControls

const props = defineProps<{ path: string | null }>()

/**
 * The scene is painted in WebGL, so it needs its own palette table instead of CSS variables.
 * The viewport takes the very neutral the card's wells are painted with (`ink-800`) so it reads as
 * one more recess on the same surface family instead of a panel of its own, and the grid is drawn
 * in the neutral stroke ramp (stroke2 on ink, stroke1 on paper) like every hairline in the UI. The
 * brand colour stays out of the scene: nothing here is interactive, and Fluent spends colour on
 * things that are. The fallback material runs the other way — light on ink, dark on paper — so the
 * model never fades into the backdrop.
 */
const SCENE_PALETTE = {
  dark: {
    backdrop: '#1f1f1f',
    grid: 0x525252,
    gridOpacity: 0.9,
    material: 0xd6d6d6,
  },
  light: {
    backdrop: '#fafafa',
    grid: 0xd1d1d1,
    gridOpacity: 0.9,
    material: 0x616161,
  },
} as const

const scenePalette = computed(() => SCENE_PALETTE[theme.value])

const viewport = ref<HTMLDivElement | null>(null)
const loading = ref(false)
/** none = normal; noMesh = the asset has no renderable mesh; failed = conversion/read error */
const errorKind = ref<'none' | 'noMesh' | 'failed'>('none')

/** Cached GLB payload (Base64) per asset path, so switching assets does not re-convert */
const cache = new Map<string, string>()
const CACHE_LIMIT = 10
/** Monotonic token: stale in-flight loads skip rendering after quick asset switches */
let loadSeq = 0

let core: ThreeModule | null = null
let Gltf: GltfCtor | null = null
let Orbit: OrbitCtor | null = null

let renderer: THREE.WebGLRenderer | null = null
let scene: THREE.Scene | null = null
let camera: THREE.PerspectiveCamera | null = null
let controls: InstanceType<OrbitCtor> | null = null
let observer: ResizeObserver | null = null
let model: THREE.Object3D | null = null
let grid: THREE.Object3D | null = null
let frameHandle = 0

async function ensureThree() {
  if (core && Gltf && Orbit) return
  const [threeMod, gltfMod, orbitMod] = await Promise.all([
    import('three'),
    import('three/examples/jsm/loaders/GLTFLoader.js'),
    import('three/examples/jsm/controls/OrbitControls.js'),
  ])
  core = threeMod
  Gltf = gltfMod.GLTFLoader
  Orbit = orbitMod.OrbitControls
}

function base64ToBytes(base64: string): Uint8Array {
  const binary = atob(base64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i)
  return bytes
}

function remember(key: string, value: string) {
  cache.delete(key)
  cache.set(key, value)
  while (cache.size > CACHE_LIMIT) {
    const oldest = cache.keys().next()
    if (oldest.done) break
    cache.delete(oldest.value)
  }
}

async function initScene() {
  const host = viewport.value
  if (!host || renderer) return

  await ensureThree()
  // The component may have unmounted while three was loading
  if (!viewport.value || !core || !Orbit) return

  renderer = new core.WebGLRenderer({antialias: true, alpha: true})
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
  renderer.setSize(host.clientWidth || 1, host.clientHeight || 1)
  host.appendChild(renderer.domElement)

  scene = new core.Scene()
  camera = new core.PerspectiveCamera(
      45,
      (host.clientWidth || 1) / (host.clientHeight || 1),
      0.01,
      5000,
  )

  const orbit = new Orbit(camera, renderer.domElement)
  orbit.enableDamping = true
  orbit.dampingFactor = 0.08
  orbit.enablePan = false
  controls = orbit

  observer = new ResizeObserver(() => resize())
  observer.observe(host)

  loop()
}

function resize() {
  const host = viewport.value
  if (!host || !renderer || !camera) return
  const w = host.clientWidth
  const h = host.clientHeight
  if (w === 0 || h === 0) return
  renderer.setSize(w, h)
  camera.aspect = w / h
  camera.updateProjectionMatrix()
}

function loop() {
  frameHandle = requestAnimationFrame(loop)
  if (!renderer || !scene || !camera) return
  controls?.update()
  renderer.render(scene, camera)
}

/** Release a material (or material array) once nothing references it anymore */
function disposeMaterial(material: THREE.Material | THREE.Material[] | undefined) {
  if (Array.isArray(material)) material.forEach((mat) => mat?.dispose())
  else material?.dispose()
}

/** Dispose the model and all GPU resources to avoid leaks on switch/unmount */
function clearModel() {
  if (!model || !scene) return
  scene.remove(model)
  model.traverse((obj) => {
    const mesh = obj as THREE.Mesh
    if (!mesh.isMesh) return
    mesh.geometry?.dispose()
    disposeMaterial(mesh.material)
  })
  model = null
}

/** Drop the reference grid so the next model does not stack another one */
function removeGrid() {
  if (!grid || !scene) return
  scene.remove(grid)
  const helper = grid as unknown as THREE.LineSegments
  helper.geometry?.dispose()
  disposeMaterial(helper.material)
  grid = null
}

function renderGlb(bytes: Uint8Array) {
  if (!scene || !camera || !core || !Gltf) return
  const orbit = controls

  // Copy into a standalone ArrayBuffer: Uint8Array.buffer is typed ArrayBufferLike (could be a
  // SharedArrayBuffer), while GLTFLoader.parse only accepts an ArrayBuffer
  const buffer = new ArrayBuffer(bytes.byteLength)
  new Uint8Array(buffer).set(bytes)

  new Gltf().parse(
      buffer,
      '',
      (gltf) => {
        if (!scene || !camera || !core) return
        clearModel()
        removeGrid()
        model = gltf.scene

        // Uniform unlit neutral-gray material: no maps and not affected by normals/lighting,
        // so broken normals or missing textures can never turn the model black.
        model.traverse((obj) => {
          const mesh = obj as THREE.Mesh
          if (!mesh.isMesh) return
          // Replace the GLB's own material, then release it: GLTF materials can be shared between
          // meshes, but every mesh is re-materialized here, so nothing keeps referencing it
          const parsed = mesh.material
          mesh.material = new core!.MeshBasicMaterial({color: scenePalette.value.material})
          disposeMaterial(parsed)
        })

        const box = new core.Box3().setFromObject(model)
        const size = box.getSize(new core.Vector3())
        const center = box.getCenter(new core.Vector3())
        const maxDim = Math.max(size.x, size.y, size.z) || 1

        // Center the model at the origin and fit the camera from the bounding-sphere distance
        // (~80% of the viewport height)
        model.position.sub(center)
        scene.add(model)

        const radius = maxDim / 2
        const dist = (radius / Math.tan((camera.fov * Math.PI) / 360)) * 1.25
        camera.position.set(dist * 0.75, dist * 0.6, dist)
        camera.near = Math.max(dist / 1000, 0.001)
        camera.far = dist * 100
        camera.updateProjectionMatrix()
        if (orbit) {
          orbit.target.set(0, 0, 0)
          orbit.minDistance = radius * 0.6
          orbit.maxDistance = dist * 6
          orbit.update()
        }

        // Reference grid under the model so it does not float in the void
        const palette = scenePalette.value
        const gridHelper = new core.GridHelper(maxDim * 3, 12, palette.grid, palette.grid)
        const gridMat = gridHelper.material as THREE.LineBasicMaterial
        gridMat.transparent = true
        // Grid colours live in vertex colors; dropping them lets the material colour drive every
        // line at once, which is what a palette swap needs
        gridMat.vertexColors = false
        gridMat.color.setHex(palette.grid)
        gridMat.opacity = palette.gridOpacity
        gridHelper.position.y = -size.y / 2
        scene.add(gridHelper)
        grid = gridHelper
      },
      () => {
        errorKind.value = 'failed'
      },
  )
}

async function load(path: string) {
  const seq = ++loadSeq
  errorKind.value = 'none'

  try {
    let entry: string | undefined = cache.get(path)
    const needFetch = entry === undefined
    if (needFetch) loading.value = true

    if (needFetch) {
      const preview: ModelPreview = await getVisualPreview(path)
      entry = preview.base64
      remember(path, entry)
    }
    // Defensive: TypeScript cannot narrow `entry` after the conditional fetch above.
    if (entry === undefined) return
    // The user may have switched assets while converting
    if (props.path !== path || seq !== loadSeq) return

    if (!renderer) {
      loading.value = true
      await initScene()
      if (props.path !== path || seq !== loadSeq) return
    }

    renderGlb(base64ToBytes(entry))
  } catch (err) {
    console.error('[get_visual_preview]', err)
    const reason = String(err)
    errorKind.value = reason.includes('No meshes found') ? 'noMesh' : 'failed'
  } finally {
    if (props.path === path && seq === loadSeq) loading.value = false
  }
}

watch(
    () => props.path,
    (path) => {
      clearModel()
      removeGrid()
      if (path) void load(path)
    },
)

/** A palette swap only recolours what is on screen: reloading would throw away the cached GLB and
 *  the camera angle the user orbited to */
watch(scenePalette, (palette) => {
  if (model) {
    model.traverse((obj) => {
      const mesh = obj as THREE.Mesh
      if (!mesh.isMesh) return
      const mat = mesh.material as THREE.MeshBasicMaterial
      mat.color.setHex(palette.material)
    })
  }
  if (grid) {
    const gridMat = (grid as THREE.GridHelper).material as THREE.LineBasicMaterial
    gridMat.color.setHex(palette.grid)
    gridMat.opacity = palette.gridOpacity
  }
})

onMounted(() => {
  // The viewport is now attached: initialize three and load the first asset here instead of in the
  // watcher (which runs before mount, when the host element does not exist yet)
  if (props.path) void load(props.path)
})

onBeforeUnmount(() => {
  cancelAnimationFrame(frameHandle)
  observer?.disconnect()
  clearModel()
  removeGrid()
  controls?.dispose()
  // dispose() frees this renderer's GPU objects but keeps the WebGL context alive; WebView2 only
  // tolerates about 16 live contexts before discarding the oldest, so a context left behind on every
  // unmount shows up as a black preview after enough asset/route switches. Losing the context is
  // what actually releases it.
  renderer?.dispose()
  renderer?.forceContextLoss()
  const host = viewport.value
  if (host && renderer) host.removeChild(renderer.domElement)
  renderer = null
  scene = null
  camera = null
  controls = null
  observer = null
})
</script>

<template>
  <div class="space-y-2">
    <div class="flex items-center justify-between">
      <p class="text-xs font-semibold text-muted">
        {{ $t('preview.title') }}
      </p>
    </div>

    <div
        ref="viewport"
        :class="loading || errorKind !== 'none' ? '' : 'cursor-grab active:cursor-grabbing'"
        :style="{background: scenePalette.backdrop}"
        class="relative h-[220px] w-full overflow-hidden rounded-md border border-hairline-strong"
    >
      <div
          v-if="loading"
          class="absolute inset-0 flex flex-col items-center justify-center gap-2 animate-fade-in"
      >
        <Loader2 class="h-5 w-5 animate-spin text-accent"/>
        <span class="text-[11px] text-muted">{{ $t('preview.loading') }}</span>
      </div>

      <div
          v-else-if="errorKind !== 'none'"
          class="absolute inset-0 flex flex-col items-center justify-center gap-2 px-4 text-center animate-fade-in"
      >
        <Box class="h-5 w-5 text-faint"/>
        <span class="text-[11px] leading-relaxed text-muted">
          {{ errorKind === 'noMesh' ? $t('preview.noMesh') : $t('preview.failed') }}
        </span>
      </div>
    </div>
  </div>
</template>
