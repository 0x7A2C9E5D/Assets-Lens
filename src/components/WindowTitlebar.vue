<script lang="ts" setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {Minus, Moon, Square, Sun, X} from 'lucide-vue-next'
import LocaleSwitcher from './LocaleSwitcher.vue'
import {
  closeWindow,
  isWindowMaximized,
  minimizeWindow,
  onWindowResized,
  startWindowDrag,
  toggleMaximizeWindow,
} from '../api/tauri'
import {theme, toggleTheme} from '../utils/theme'

const isMaximized = ref(false)
let unlistenResize: (() => void) | null = null

async function refreshMaximized() {
  isMaximized.value = await isWindowMaximized()
}

async function handleMinimize() {
  await minimizeWindow()
}

async function handleToggleMaximize() {
  await toggleMaximizeWindow()
  await refreshMaximized()
}

async function handleClose() {
  await closeWindow()
}

/** Interactive controls inside the title bar (clicks must not trigger dragging / double-click maximize) */
const INTERACTIVE_SELECTOR =
    'button, a, input, select, [role="combobox"], [role="listbox"], [data-no-drag]'

/** Pressing the left button on any blank/text area of the title bar drags the window;
 *  startDragging must be triggered by a real mouse event */
function handleHeaderMouseDown(e: MouseEvent) {
  if (e.button !== 0) return
  const target = e.target as HTMLElement
  if (target.closest(INTERACTIVE_SELECTOR)) return
  e.preventDefault()
  void startWindowDrag()
}

/** Double-clicking a blank area of the title bar toggles maximize / restore */
function handleHeaderDoubleClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  if (target.closest(INTERACTIVE_SELECTOR)) return
  void handleToggleMaximize()
}

onMounted(async () => {
  await refreshMaximized()
  // Window resize events already fire on maximize / restore, so no polling is needed
  unlistenResize = await onWindowResized(refreshMaximized)
})

onUnmounted(() => {
  unlistenResize?.()
})
</script>

<template>
  <header
      class="relative z-30 flex h-12 shrink-0 select-none items-center justify-between border-b border-hairline bg-page text-fg"
      @dblclick="handleHeaderDoubleClick"
      @mousedown="handleHeaderMouseDown"
  >
    <!-- Spacer: pushes the toolbar right, and doubles as the drag surface of the title bar -->
    <div class="flex h-full flex-1"/>

    <!-- Right toolbar: locale switcher, palette toggle, divider, window controls -->
    <div class="flex h-full items-center gap-1 px-4">
      <LocaleSwitcher/>

      <!-- Palette toggle: the icon shows what the click leads to, so a dark bar offers the sun -->
      <button
          :aria-label="theme === 'dark' ? $t('window.themeToLight') : $t('window.themeToDark')"
          :title="theme === 'dark' ? $t('window.themeToLight') : $t('window.themeToDark')"
          class="flex h-8 w-8 items-center justify-center rounded-md text-muted transition-colors duration-150 ease-fluent hover:bg-tint hover:text-fg"
          type="button"
          @click="toggleTheme"
      >
        <Sun v-if="theme === 'dark'" class="h-4 w-4"/>
        <Moon v-else class="h-4 w-4"/>
      </button>

      <!-- Divider between the toolbar and the window controls -->
      <div aria-hidden="true" class="mx-1 h-5 w-px bg-hairline-strong"/>

      <button
          :aria-label="$t('window.minimize')"
          :title="$t('window.minimize')"
          class="flex h-8 w-8 items-center justify-center rounded-md text-muted transition-colors duration-150 ease-fluent hover:bg-tint hover:text-fg"
          type="button"
          @click="handleMinimize"
      >
        <Minus class="h-3.5 w-3.5"/>
      </button>
      <button
          :aria-label="isMaximized ? $t('window.restore') : $t('window.maximize')"
          :title="isMaximized ? $t('window.restore') : $t('window.maximize')"
          class="flex h-8 w-8 items-center justify-center rounded-md text-muted transition-colors duration-150 ease-fluent hover:bg-tint hover:text-fg"
          type="button"
          @click="handleToggleMaximize"
      >
        <!-- Maximize: single box -->
        <Square v-if="!isMaximized" class="h-3.5 w-3.5"/>
        <!-- Restore: two stacked boxes -->
        <svg
            v-else
            aria-hidden="true"
            class="h-3.5 w-3.5"
            fill="none"
            stroke="currentColor"
            stroke-linejoin="round"
            stroke-width="1.4"
            viewBox="0 0 16 16"
        >
          <rect height="8.75" rx="0.75" width="8.75" x="1.75" y="5.5"/>
          <rect height="8.75" rx="0.75" width="8.75" x="5.5" y="1.75"/>
        </svg>
      </button>
      <button
          :aria-label="$t('window.close')"
          :title="$t('window.close')"
          class="flex h-8 w-8 items-center justify-center rounded-md text-muted transition-colors duration-150 ease-fluent hover:bg-close hover:text-on-brand"
          type="button"
          @click="handleClose"
      >
        <X class="h-4 w-4"/>
      </button>
    </div>
  </header>
</template>
