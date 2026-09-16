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

/** Sidebar link styling: every entry shares the same base and active appearance. The row centers its
 *  children, so the icon lines up with the middle of the two-line label / hint block. Selection is
 *  the Fluent NavigationView treatment — a subtle fill plus the brand accent bar on the left edge,
 *  with the icon picking up the accent colour — and nothing moves on hover. */
const NAV_LINK_CLASS =
    'group flex items-center gap-3 rounded-md px-3 py-2 text-muted transition-colors duration-150 ease-fluent hover:bg-tint hover:text-fg'
const ACTIVE_CLASS =
    '!bg-tint !text-fg shadow-[inset_3px_0_0_0_rgb(var(--c-accent))] [&>svg]:text-accent'

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
  <div class="flex h-screen w-screen overflow-hidden bg-page">
    <aside
        class="z-20 flex h-full w-[300px] shrink-0 flex-col border-r border-hairline-strong bg-ink-800"
    >
      <!-- Sidebar top: icon (spans both rows, centered with the text block) + app name + subtitle (window drag area).
           It is exactly as tall as the title bar so the two columns share one horizontal line -->
      <div
          class="flex h-12 select-none items-center gap-2.5 px-4"
          @mousedown.left.prevent="handleSidebarDrag"
          @dblclick.prevent="handleSidebarDoubleClick"
      >
        <Aperture class="h-6 w-6 shrink-0 text-accent"/>
        <div class="flex flex-col leading-tight">
          <p class="text-sm font-semibold text-fg">
            {{ $t('app.title') }}
          </p>
          <p class="text-[11px] text-muted">{{ $t('app.subtitle') }}</p>
        </div>
      </div>

      <nav class="flex flex-1 flex-col gap-0.5 px-2 pt-3">
        <RouterLink
            v-for="item in navItems"
            :key="item.to"
            :active-class="ACTIVE_CLASS"
            :class="NAV_LINK_CLASS"
            :to="item.to"
        >
          <component :is="item.icon" class="h-6 w-6 shrink-0"/>
          <span class="flex flex-col">
            <span class="text-sm font-medium">{{ item.label }}</span>
            <span class="text-[11px] text-muted">{{ item.hint }}</span>
          </span>
        </RouterLink>
      </nav>

      <!-- Sidebar bottom: pinned row for secondary links -->
      <div class="flex flex-col gap-0.5 border-t border-hairline-strong px-2 pb-3 pt-3">
        <RouterLink
            v-for="item in aboutItems"
            :key="item.to"
            :active-class="ACTIVE_CLASS"
            :class="NAV_LINK_CLASS"
            :to="item.to"
        >
          <component :is="item.icon" class="h-6 w-6 shrink-0"/>
          <span class="flex flex-col">
            <span class="text-sm font-medium">{{ item.label }}</span>
            <span class="text-[11px] text-muted">{{ item.hint }}</span>
          </span>
        </RouterLink>
      </div>
    </aside>

    <!-- Main area: fixed 72px title bar, body takes the remaining space. The title bar lives outside
         <main>, so it never moves while the body scrolls -->
    <div class="flex min-h-0 min-w-0 flex-1 flex-col">
      <WindowTitlebar/>

      <!-- Main content: page-level scrolling disabled; long content (tables/details) scrolls inside its own component -->
      <main class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-page">
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
