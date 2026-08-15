import path from "path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import babel from "@rolldown/plugin-babel"
import { lingui, linguiTransformerBabelPreset } from "@lingui/vite-plugin"
import { defineConfig } from "vite"

/**
 * The member area — a second, small bundle.
 *
 * Not a route inside the panel. The panel is mounted at `/admin`, is told by
 * nginx to be `noindex, nofollow`, and calls itself Mavi CMS; a student
 * opening a course they paid for should not be at the administrator's address
 * reading the CMS's name. So this is its own entry, its own base, and its own
 * bundle, and it carries none of the editor.
 *
 * It has no router plugin because it has four screens and one of them is
 * reached by a link in an email. A path switch is smaller than a route tree
 * and does not generate a file that has to be kept in sync.
 */
/**
 * In development, Vite's fallback for an address it does not recognise is the
 * project's `index.html` — which is the panel, whose router then sends the
 * browser to `/admin`. So `/learn/anything` has to be pointed at this entry
 * instead.
 *
 * It runs before Vite's own middlewares, which is what makes it a rewrite
 * rather than a fallback — with `appType: "mpa"` there is no fallback to be
 * later than.
 */
function serveLearnInDev() {
  return {
    name: "learn-fallback",
    configureServer(server: { middlewares: { use: (fn: unknown) => void } }) {
      server.middlewares.use(
        (
          request: { url?: string; headers: Record<string, string | undefined> },
          _response: unknown,
          next: () => void
        ) => {
          // Whether the browser is asking for a page, rather than for a module
          // or an asset. Testing the path instead does not work: `@vite/client`
          // has no file extension either, and rewriting it hands HTML to the
          // module transformer, which reads it as JavaScript and fails on the
          // first comment.
          const wantsAPage = request.headers.accept?.includes("text/html")
          if (wantsAPage && (request.url ?? "").startsWith("/learn")) {
            request.url = "/learn/learn.html"
          }
          next()
        }
      )
    },
  }
}

export default defineConfig({
  base: "/learn/",
  // No SPA fallback: Vite's would be the project's `index.html`, which is the
  // panel. The middleware above points a member-area address at this entry,
  // and anything else is honestly a 404 rather than quietly the wrong app.
  appType: "mpa",
  // And this is what to scan when pre-bundling, or dev spends its first thirty
  // seconds optimising Tiptap and the flow canvas for a page that uses
  // neither.
  optimizeDeps: { entries: ["learn.html"] },
  build: {
    outDir: "dist-learn",
    emptyOutDir: true,
    // Written but not advertised: the .js carries no sourceMappingURL, so a
    // browser never asks for the map. The image build uploads them to the
    // error tracker if one is configured, and deletes them either way.
    sourcemap: "hidden",
    // Named, because Vite's default entry is `index.html` — which is the
    // panel. Without this the member area builds the whole editor, the flow
    // canvas and both message catalogues into one file and calls it a course
    // page: 2.7 MB rather than 250 KB, measured.
    //
    // The emitted page keeps the entry's name, `learn.html`, which is what
    // nginx falls back to. Renaming it to `index.html` would mean renaming the
    // source, and a file called `index.html` next to the panel's own is worth
    // less than one line in nginx.conf.
    rollupOptions: { input: path.resolve(__dirname, "learn.html") },
  },
  plugins: [
    serveLearnInDev(),
    react(),
    tailwindcss(),
    lingui(),
    babel({ presets: [linguiTransformerBabelPreset()] }),
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 5174,
    proxy: {
      "/api": {
        target: process.env.VITE_API_PROXY_TARGET ?? "http://localhost:8080",
        changeOrigin: true,
        rewrite: (requestPath) => requestPath.replace(/^\/api/, ""),
      },
    },
  },
})
