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
    // Dev-only proxy plugin to bypass CORS for subscription fetches and ping checks.
    {
      name: 'cors-proxy',
      configureServer(server: import('vite').ViteDevServer) {
        const isSafeHttpUrl = (value: string) => {
          try {
            const parsed = new URL(value);
            return parsed.protocol === 'http:' || parsed.protocol === 'https:';
          } catch {
            return false;
          }
        };

        server.middlewares.use('/api/proxy', async (req, res) => {
          const url = new URL(req.url || '', 'http://localhost').searchParams.get('url');
          if (!url || !isSafeHttpUrl(url)) {
            res.writeHead(400);
            res.end('Missing or invalid url param');
            return;
          }
          try {
            const response = await fetch(url);
            const subscriptionUserinfo =
              response.headers.get('subscription-userinfo') ||
              response.headers.get('x-subscription-userinfo') ||
              '';
            const profileTitle =
              response.headers.get('profile-title') ||
              response.headers.get('x-profile-title') ||
              '';
            const contentDisposition = response.headers.get('content-disposition') || '';
            const text = await response.text();
            res.writeHead(response.status, {
              'Content-Type': response.headers.get('content-type') || 'text/plain',
              'Access-Control-Allow-Origin': '*',
              'Access-Control-Expose-Headers': 'subscription-userinfo,x-subscription-userinfo,profile-title,x-profile-title,content-disposition',
              'Cache-Control': 'no-store',
              ...(subscriptionUserinfo ? { 'subscription-userinfo': subscriptionUserinfo } : {}),
              ...(profileTitle ? { 'profile-title': profileTitle } : {}),
              ...(contentDisposition ? { 'content-disposition': contentDisposition } : {}),
            });
            res.end(text);
          } catch (err) {
            res.writeHead(500);
            res.end(String(err));
          }
        });

        server.middlewares.use('/api/ping', async (req, res) => {
          const params = new URL(req.url || '', 'http://localhost').searchParams;
          const address = params.get('address') || '';
          const port = Number(params.get('port') || '0');
          if (!address || !Number.isInteger(port) || port <= 0 || port > 65535) {
            res.writeHead(400, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ ping_ms: -1 }));
            return;
          }

          try {
            const net = await import('node:net');
            const startedAt = Date.now();
            const pingMs = await new Promise<number>((resolve) => {
              const socket = net.createConnection({ host: address, port, timeout: 3000 });
              const finish = (value: number) => {
                socket.removeAllListeners();
                socket.destroy();
                resolve(value);
              };
              socket.once('connect', () => finish(Date.now() - startedAt));
              socket.once('timeout', () => finish(-1));
              socket.once('error', () => finish(-1));
            });
            res.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
            res.end(JSON.stringify({ ping_ms: pingMs }));
          } catch {
            res.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
            res.end(JSON.stringify({ ping_ms: -1 }));
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
