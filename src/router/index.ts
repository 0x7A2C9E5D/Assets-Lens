import {createRouter, createWebHashHistory} from 'vue-router'

/** Every route imports its view on demand, so startup only pays for the shell and the current page. */
export default createRouter({
    history: createWebHashHistory(),
    routes: [
        {path: '/', redirect: '/database'},
        {path: '/database', name: 'database', component: () => import('../views/DatabaseView.vue')},
        {path: '/browse', name: 'browse', component: () => import('../views/BrowseView.vue')},
        {path: '/about', name: 'about', component: () => import('../views/AboutView.vue')},
    ],
})
