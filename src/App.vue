<script lang="ts" setup>
import {computed} from 'vue'
import {Aperture, Boxes, Database, Info} from 'lucide-vue-next'
import {useI18n} from 'vue-i18n'
import WindowTitlebar from './components/WindowTitlebar.vue'
import {startWindowDrag, toggleMaximizeWindow} from './api/tauri'

const {t} = useI18n()

/** The sidebar top is also a window drag area (the sidebar is fixed and independent of WindowTitlebar, so dragging must be bound here) */
function handleSidebarDrag(e: MouseEvent) {
  if (e.button !== 0) return
  void startWindowDrag()
}

function handleSidebarDoubleClick() {
  void toggleMaximizeWindow()
}

/** Sidebar link styling: every entry shares the same base and active appearance */
const NAV_LINK_CLASS =
    'group flex items-start gap-3 rounded-xl px-3 py-3 text-muted transition-all duration-200 hover:translate-x-1 hover:bg-white/5 hover:text-[#E6EDF7]'
const ACTIVE_CLASS =
    '!bg-gradient-to-r !from-glow-cyan/20 !to-glow-blue/10 !text-glow-cyan shadow-[inset_0_0_0_1px_rgba(34,211,238,0.35)]'

const navItems = computed(() => [
  {to: '/database', label: t('nav.database'), hint: t('nav.databaseHint'), icon: Database},
  {to: '/browse', label: t('nav.browse'), hint: t('nav.browseHint'), icon: Boxes},
])

/** Pinned to the bottom of the sidebar, separated from the main navigation */
const aboutItems = computed(() => [
  {to: '/about', label: t('nav.about'), hint: t('nav.aboutHint'), icon: Info},
])
</script>

<template>
  <!-- Fill the whole window: 300px sidebar + adaptive main area; every window size fills completely, no central canvas and no gutters -->
  <div class="flex h-screen w-screen overflow-hidden bg-ink-900">
    <aside
        class="z-20 flex h-full w-[300px] shrink-0 flex-col border-r border-cyan-300/10 bg-ink-900/80 backdrop-blur-xl"
    >
      <!-- Sidebar top: icon (spans both rows, centered with the text block) + app name + subtitle (window drag area) -->
      <div
          class="flex h-[72px] select-none items-center gap-2.5 px-6"
          @mousedown.left.prevent="handleSidebarDrag"
          @dblclick.prevent="handleSidebarDoubleClick"
      >
        <Aperture class="h-8 w-8 shrink-0 text-glow-cyan"/>
        <div class="flex flex-col">
          <p class="text-base font-semibold tracking-wide text-[#E6EDF7]">
            {{ $t('app.title') }}
          </p>
          <p class="text-[11px] text-muted/70">{{ $t('app.subtitle') }}</p>
        </div>
      </div>

      <nav class="flex flex-1 flex-col gap-2 px-3 pt-4">
        <RouterLink
            v-for="item in navItems"
            :key="item.to"
            :active-class="ACTIVE_CLASS"
            :class="NAV_LINK_CLASS"
            :to="item.to"
        >
          <component :is="item.icon" class="mt-0.5 h-4 w-4 shrink-0"/>
          <span class="flex flex-col">
            <span class="text-sm font-medium">{{ item.label }}</span>
            <span class="text-[11px] text-muted/70">{{ item.hint }}</span>
          </span>
        </RouterLink>
      </nav>

      <!-- Sidebar bottom: pinned row for secondary links -->
      <div class="flex flex-col gap-2 border-t border-cyan-300/10 px-3 pb-4 pt-3">
        <RouterLink
            v-for="item in aboutItems"
            :key="item.to"
            :active-class="ACTIVE_CLASS"
            :class="NAV_LINK_CLASS"
            :to="item.to"
        >
          <component :is="item.icon" class="mt-0.5 h-4 w-4 shrink-0"/>
          <span class="flex flex-col">
            <span class="text-sm font-medium">{{ item.label }}</span>
            <span class="text-[11px] text-muted/70">{{ item.hint }}</span>
          </span>
        </RouterLink>
      </div>
    </aside>

    <!-- Main area: fixed 72px title bar, body takes the remaining space. The title bar lives outside
         <main>, so it never moves while the body scrolls -->
    <div class="flex min-h-0 min-w-0 flex-1 flex-col">
      <WindowTitlebar/>

      <!-- Main content: page-level scrolling disabled; long content (tables/details) scrolls inside its own component -->
      <main class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-ink-900">
        <!-- Note: do not wrap RouterView in <Transition>. In WebView2 an out-in transition gets
             stuck between leave/enter during route changes and renders a blank screen (with no
             console error). Render directly and bind :key so every route change cleanly rebuilds
             the page component. -->
        <RouterView v-slot="{ Component }">
          <component :is="Component" :key="$route.fullPath"/>
        </RouterView>
      </main>
    </div>
  </div>
</template>
