import tailwindcss from '@tailwindcss/vite'

export default defineNuxtConfig({
  compatibilityDate: '2025-05-02',

  future: {
    compatibilityVersion: 4,
  },

  // SPA, not SSG. Everything on the page is client-only — Monaco touches `window` at import
  // time and the compiler is a WebAssembly module in a worker — and there is no content to
  // prerender. `ssr: false` means `nuxt generate` emits a single static shell, which removes
  // every `<ClientOnly>` guard the SSR path would otherwise need.
  ssr: false,

  modules: ['shadcn-nuxt'],

  shadcn: {
    prefix: '',
    componentDir: './app/components/ui',
  },

  components: [{ path: '~/components', pathPrefix: false }],

  css: ['~/assets/css/main.css'],

  app: {
    head: {
      htmlAttrs: { lang: 'en-AU' },
      link: [{ rel: 'icon', type: 'image/svg+xml', href: '/icon.svg' }],
    },
  },

  vite: {
    plugins: [tailwindcss()],

    worker: {
      // The compiler worker is an ES module: it uses a dynamic `import()` to load the
      // wasm-bindgen glue, and the glue in turn resolves the `.wasm` through
      // `new URL(..., import.meta.url)`. Neither works under the default `iife` worker
      // format.
      format: 'es',
    },

    optimizeDeps: {
      // Both Monaco entries are listed explicitly because they are reached through dynamic
      // `import()` inside a component, which Vite's dependency scanner does not follow. Left
      // undiscovered, the first page load triggers a mid-flight re-optimise and the in-flight
      // route chunk 504s ("Outdated Optimize Dep").
      include: [
        // Reached through dynamic `import()` inside a component, which Vite's dependency
        // scanner does not follow.
        'monaco-editor/esm/vs/editor/editor.api.js',
        'monaco-editor/esm/vs/editor/edcore.main.js',
        // Listed for a different reason: Nuxt clears `.nuxt/dist` and restarts on a cold
        // start, so Vite discovers these mid-request, re-optimises, and 504s the route chunk
        // that was already in flight. Naming them up front makes the first `pnpm dev` load
        // clean instead of self-healing after an error page.
        '@lucide/vue',
        '@vueuse/core',
        'class-variance-authority',
        'clsx',
        'effect',
        'reka-ui',
        'tailwind-merge',
      ],
    },
  },

  devServer: {
    port: 4200,
  },

  devtools: { enabled: true },
})
