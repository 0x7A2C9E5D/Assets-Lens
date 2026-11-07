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
 * The scene needs its own palette table instead of CSS variables. The backdrop matches the card's
 * wells, the grid uses the neutral stroke ramp, and the lit clay material is nudged per theme so a
 * texture-less mesh still reads by its shading.
 */
const SCENE_PALETTE = {
  dark: {
    backdrop: '#1f1f1f',
    grid: 0x525252,
    gridOpacity: 0.9,
    clay: 0x969696,
  },
  light: {
    backdrop: '#fafafa',
    grid: 0xd1d1d1,
    gridOpacity: 0.9,
    clay: 0xacacac,
  },
} as const

/**
 * The rig's tones, identical in both themes; `sky` / `ground` fill the shadow side, `key` / `rim`
 * shade the lit side.
 */
const LIGHT_RIG = {
  sky: 0xffffff,
  ground: 0xd6d6d6,
  key: 0xffffff,
  rim: 0xffffff,
} as const

/**
 * One fixed rig for every asset: key up-right, rim behind-left, hemisphere filling the shadows.
 * Intensities use three's physically-correct units, peaking just above 1.0.
 */
const SCENE_LIGHT = {hemisphere: 1.3, key: 2.0, rim: 0.9} as const

/** Roughness of the clay: enough sheen for a specular roll-off, not a highlight */
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

  // Lights are scene children: every asset shares the rig, and it must outlive the previous model
  const key = new core.DirectionalLight(LIGHT_RIG.key, SCENE_LIGHT.key)
  key.position.set(2, 3, 2.5)
  const rim = new core.DirectionalLight(LIGHT_RIG.rim, SCENE_LIGHT.rim)
  rim.position.set(-2.5, 1, -2)
  const hemisphere = new core.HemisphereLight(
      LIGHT_RIG.sky,
      LIGHT_RIG.ground,
      SCENE_LIGHT.hemisphere,
  )
  scene.add(key, rim, hemisphere)

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

/** Triangles of one mesh, the yardstick for which copy of a part is the finest */
function triangleCount(mesh: THREE.Mesh): number {
  const geometry = mesh.geometry
  const count = geometry.index ? geometry.index.count : geometry.attributes.position.count
  return Math.floor(count / 3)
}

/**
 * A GR2 carries every LOD; maclarian copies them all into the GLB, so the scene draws decimated
 * copies on top of the real surface. Keep the finest copy of each part.
 */
function keepFinestLod(model: THREE.Object3D) {
  const finest = new Map<string, THREE.Mesh>()
  model.traverse((obj) => {
    const mesh = obj as THREE.Mesh
    if (!mesh.isMesh) return
    const part = mesh.name.replace(/_LOD\d+$/i, '')
    const current = finest.get(part)
    if (!current || triangleCount(mesh) > triangleCount(current)) finest.set(part, mesh)
  })

  const keep = new Set(finest.values())
  const dropped: THREE.Mesh[] = []
  model.traverse((obj) => {
    const mesh = obj as THREE.Mesh
    if (mesh.isMesh && !keep.has(mesh)) dropped.push(mesh)
  })
  for (const mesh of dropped) {
    mesh.removeFromParent()
    mesh.geometry.dispose()
  }
}

/**
 * Some meshes have vertex normals pointing into the surface. Flip a vertex whose normal opposes
 * the area-weighted face normals around it, which keeps the authored smoothing.
 */
function repairInvertedNormals(geometry: THREE.BufferGeometry) {
  const position = geometry.attributes.position as THREE.BufferAttribute | undefined
  const normal = geometry.attributes.normal as THREE.BufferAttribute | undefined
  if (!position || !normal) return

  const index = geometry.index
  const count = index ? index.count : position.count
  // Area-weighted sum of face normals per vertex (cross length is twice the area, so no normalize)
  const faces = new Float32Array(position.count * 3)
  for (let i = 0; i + 2 < count; i += 3) {
    const a = index ? index.getX(i) : i
    const b = index ? index.getX(i + 1) : i + 1
    const c = index ? index.getX(i + 2) : i + 2
    const ax = position.getX(a)
    const ay = position.getY(a)
    const az = position.getZ(a)
    const ux = position.getX(b) - ax
    const uy = position.getY(b) - ay
    const uz = position.getZ(b) - az
    const vx = position.getX(c) - ax
    const vy = position.getY(c) - ay
    const vz = position.getZ(c) - az
    const nx = uy * vz - uz * vy
    const ny = uz * vx - ux * vz
    const nz = ux * vy - uy * vx
    for (const v of [a, b, c]) {
      faces[v * 3] += nx
      faces[v * 3 + 1] += ny
      faces[v * 3 + 2] += nz
    }
  }

  for (let v = 0; v < position.count; v += 1) {
    const gx = faces[v * 3]
    const gy = faces[v * 3 + 1]
    const gz = faces[v * 3 + 2]
    // A vertex shared by surfaces facing both ways has no meaningful side: leave it alone
    if (gx * gx + gy * gy + gz * gz < 1e-12) continue
    const nx = normal.getX(v)
    const ny = normal.getY(v)
    const nz = normal.getZ(v)
    if (gx * nx + gy * ny + gz * nz < 0) normal.setXYZ(v, -nx, -ny, -nz)
  }
  normal.needsUpdate = true
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

        keepFinestLod(model)

        // One neutral clay material: no maps, shading from normals. DoubleSide keeps BG3's
        // single-layer hair/fur cards visible from behind (maclarian writes no `doubleSided`).
        model.traverse((obj) => {
          const mesh = obj as THREE.Mesh
          if (!mesh.isMesh) return
          repairInvertedNormals(mesh.geometry)
          // Replace the GLB's material and release it: every mesh is re-materialized below
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

        // Center the model at the origin and fit the camera from the bounding sphere
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
        // Grid colors live in vertex colors; dropping them lets the material color drive every line
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

/** Repaint the backdrop and grid and nudge the clay; the light rig keeps its colors, so no reload
 *  (which would throw away the camera angle). */
watch(scenePalette, (palette) => {
  if (model) {
    model.traverse((obj) => {
      const mesh = obj as THREE.Mesh
      if (!mesh.isMesh) return
      const mat = mesh.material as THREE.MeshStandardMaterial
      mat.color.setHex(palette.clay)
    })
  }
  if (grid) {
    const gridMat = (grid as THREE.GridHelper).material as THREE.LineBasicMaterial
    gridMat.color.setHex(palette.grid)
    gridMat.opacity = palette.gridOpacity
  }
})

onMounted(() => {
  // The viewport exists only after mount, so start the first asset here, not in the watcher
  if (props.path) void load(props.path)
})

onBeforeUnmount(() => {
  cancelAnimationFrame(frameHandle)
  observer?.disconnect()
  clearModel()
  removeGrid()
  controls?.dispose()
  // dispose() frees GPU objects but keeps the WebGL context alive; WebView2 tolerates about 16
  // live contexts, so losing the context is what actually releases it.
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
