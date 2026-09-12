import {createRouter, createWebHashHistory} from 'vue-router'

/**
 * Each route imports its own view on demand: a page's code, plus whatever it drags in (the asset
 * table, the detail panel), is fetched when that page is first opened. Startup therefore only pays
 * for the shell and the page actually being shown.
 */
export default createRouter({
    history: createWebHashHistory(),
    routes: [
        {path: '/', redirect: '/database'},
        {path: '/database', name: 'database', component: () => import('../views/DatabaseView.vue')},
        {path: '/browse', name: 'browse', component: () => import('../views/BrowseView.vue')},
        {path: '/about', name: 'about', component: () => import('../views/AboutView.vue')},
    ],
})
