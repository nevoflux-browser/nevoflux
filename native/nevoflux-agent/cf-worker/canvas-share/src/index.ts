import { Hono } from 'hono';
import { cors } from 'hono/cors';
import type { Env } from './types';
import { handleUpload } from './handlers/upload';
import { handleFetchBundle } from './handlers/fetch';
import { handleMeta } from './handlers/meta';
import { handleExtend } from './handlers/extend';
import { handleDelete } from './handlers/delete';
import { handleLanding } from './handlers/landing';

const app = new Hono<{ Bindings: Env }>();

// CORS middleware — allow NevoFlux extension requests
app.use(
  '/api/*',
  cors({
    origin: (origin) => {
      // Allow moz-extension:// origins and localhost for dev
      if (
        origin.startsWith('moz-extension://') ||
        origin.startsWith('chrome-extension://') ||
        origin.startsWith('http://localhost')
      ) {
        return origin;
      }
      return '';
    },
    allowMethods: ['GET', 'POST', 'PATCH', 'DELETE', 'OPTIONS'],
    allowHeaders: ['Content-Type', 'X-Owner-Token'],
    maxAge: 86400,
  })
);

// API routes
app.post('/api/share', handleUpload);
app.get('/api/share/:id/bundle', handleFetchBundle);
app.get('/api/share/:id/meta', handleMeta);
app.patch('/api/share/:id', handleExtend);
app.delete('/api/share/:id', handleDelete);

// Landing page
app.get('/c/:id', handleLanding);

// Health check
app.get('/health', (c) => c.json({ status: 'ok', service: 'nevoflux-canvas-share' }));

// Cron trigger handler (scheduled event for TTL cleanup)
export default {
  fetch: app.fetch,

  async scheduled(_event: ScheduledEvent, env: Env, ctx: ExecutionContext): Promise<void> {
    ctx.waitUntil(cleanupExpiredShares(env));
  },
};

/**
 * Remove expired shares from KV and R2.
 *
 * Lists all KV keys with prefix "share:" and deletes any whose
 * expires_at timestamp is in the past.
 */
async function cleanupExpiredShares(env: Env): Promise<void> {
  const now = new Date().toISOString();
  let cursor: string | undefined;

  do {
    const list: KVNamespaceListResult<unknown, string> = await env.SHARE_KV.list({
      prefix: 'share:',
      cursor,
      limit: 100,
    });

    for (const key of list.keys) {
      const metaStr = await env.SHARE_KV.get(key.name);
      if (!metaStr) continue;

      try {
        const meta = JSON.parse(metaStr) as { expires_at: string };
        if (meta.expires_at < now) {
          // Extract share_id from key "share:{share_id}"
          const shareId = key.name.replace('share:', '');

          // Delete from R2 and KV in parallel
          await Promise.all([
            env.SHARE_BUCKET.delete(`share_assets/${shareId}.bin`),
            env.SHARE_KV.delete(key.name),
          ]);
        }
      } catch {
        // Skip malformed entries
      }
    }

    cursor = list.list_complete ? undefined : list.cursor;
  } while (cursor);
}
