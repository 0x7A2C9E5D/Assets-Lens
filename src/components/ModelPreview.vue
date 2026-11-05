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
 * things that are.
 *
 * The model is painted with a lit clay material rather than a flat fill: a texture-less BG3 mesh
 * carries no albedo, so its curvature — the only detail there is to see — is shaded normals and
 * nothing else. A single neutral albedo per theme stays in charge of the tone; `sky` / `ground`
 * are the two ends of the hemisphere that fills the shadow side, and `key` / `rim` shade the lit
 * side and separate the silhouette from the backdrop. Light on ink and dark on paper, so the model
 * never fades into its well.
 */
const SCENE_PALETTE = {
  dark: {
    backdrop: '#1f1f1f',
    grid: 0x525252,
    gridOpacity: 0.9,
    clay: 0xb0b0b0,
    sky: 0xf2f2f2,
    ground: 0x8a8a8a,
    key: 0xffffff,
    rim: 0xffffff,
  },
  light: {
    backdrop: '#fafafa',
    grid: 0xd1d1d1,
    gridOpacity: 0.9,
    clay: 0x666666,
    sky: 0xffffff,
    ground: 0xb8b8b8,
    key: 0xffffff,
    rim: 0xffffff,
  },
} as const

/**
 * One fixed rig for every asset: the key sits up and to the right of the default camera, the rim
 * behind and to the left to edge the silhouette off the backdrop, and the hemisphere fills the
 * shadow side from above while its ground term keeps undersides from crushing into the backdrop.
 * Intensities are three's physically-correct units (diffuse irradiance is divided by PI again in
 * the BRDF), so the peak — hemisphere sky plus key — lands just above 1.0 and the clay is shaded
 * without clipping to white.
 */
const SCENE_LIGHT = {hemisphere: 1.3, key: 2.0, rim: 0.9} as const

/** Roughness of the clay: enough sheen to catch a specular roll-off across curvature, not enough
 *  to turn a small viewport into a highlight */
const CLAY_ROUGHNESS = 0.62

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
/** The rig lives on the scene, so it survives every asset switch and only its colours follow the theme */
let lights: {
  hemisphere: THREE.HemisphereLight
  key: THREE.DirectionalLight
  rim: THREE.DirectionalLight
} | null = null
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

  // Lights are scene children, not model children: every asset is lit by the same rig, and the
  // rig must not be swept away with the previous model
  const palette = scenePalette.value
  const key = new core.DirectionalLight(palette.key, SCENE_LIGHT.key)
  key.position.set(2, 3, 2.5)
  const rim = new core.DirectionalLight(palette.rim, SCENE_LIGHT.rim)
  rim.position.set(-2.5, 1, -2)
  const hemisphere = new core.HemisphereLight(palette.sky, palette.ground, SCENE_LIGHT.hemisphere)
  scene.add(key, rim, hemisphere)
  lights = {hemisphere, key, rim}

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

        const palette = scenePalette.value

        // One neutral clay material in place of the GLB's own: no maps, so nothing depends on a
        // texture that was never exported, but the surface is shaded from its normals, which is
        // what makes curvature, hair cards and cloth folds readable. Broken normals only lose their
        // shading here — the hemisphere light still keeps every polygon off pure black.
        //
        // DoubleSide is not an aesthetic choice: BG3 hair, fur and cloth are single-layer cards that
        // the engine's own material flag draws from both sides, while maclarian's GLB writes no
        // `doubleSided` at all, so glTF's single-sided default would cull every card seen from
        // behind. three flips the normal for a back-facing triangle, so the shading stays correct.
        model.traverse((obj) => {
          const mesh = obj as THREE.Mesh
          if (!mesh.isMesh) return
          // Replace the GLB's own material, then release it: GLTF materials can be shared between
          // meshes, but every mesh is re-materialized here, so nothing keeps referencing it
          const parsed = mesh.material
          mesh.material = new core!.MeshStandardMaterial({
            color: palette.clay,
            roughness: CLAY_ROUGHNESS,
            metalness: 0,
            side: core!.DoubleSide,
          })
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
      const mat = mesh.material as THREE.MeshStandardMaterial
      mat.color.setHex(palette.clay)
    })
  }
  if (lights) {
    lights.hemisphere.color.setHex(palette.sky)
    lights.hemisphere.groundColor.setHex(palette.ground)
    lights.key.color.setHex(palette.key)
    lights.rim.color.setHex(palette.rim)
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
  lights = null
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
