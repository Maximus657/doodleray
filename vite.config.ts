import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    react(),
    tailwindcss(),
    // Dev-only proxy plugin to bypass CORS for subscription fetches
    {
      name: 'cors-proxy',
      configureServer(server: import('vite').ViteDevServer) {
        server.middlewares.use('/api/proxy', async (req, res) => {
          const url = new URL(req.url || '', 'http://localhost').searchParams.get('url');
          if (!url) {
            res.writeHead(400);
            res.end('Missing url param');
            return;
          }
          try {
            const response = await fetch(url);
            const text = await response.text();
            res.writeHead(200, {
              'Content-Type': response.headers.get('content-type') || 'text/plain',
              'Access-Control-Allow-Origin': '*',
            });
            res.end(text);
          } catch (err) {
            res.writeHead(500);
            res.end(String(err));
          }
        });
      },
    },
  ],

  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
