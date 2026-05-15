import type { Context } from 'hono';
import type { Env, ShareKVMeta } from '../types';
import { isValidShareId } from '../utils/validation';
import { jsonError, notFound } from '../utils/responses';

/**
 * GET /api/share/:id/bundle
 *
 * Download the encrypted share bundle.
 *
 * Response:
 * - 200: Raw NFEB binary blob (application/octet-stream)
 * - 400: Invalid share ID
 * - 404: Share not found or expired
 */
export async function handleFetchBundle(c: Context<{ Bindings: Env }>): Promise<Response> {
  const shareId = c.req.param('id');

  if (!shareId || !isValidShareId(shareId)) {
    return jsonError('Invalid share ID format');
  }

  // Check metadata exists and not expired
  const metaStr = await c.env.SHARE_KV.get(`share:${shareId}`);
  if (!metaStr) {
    return notFound('Share not found or expired');
  }

  const meta: ShareKVMeta = JSON.parse(metaStr);

  // Check explicit expiration
  if (new Date(meta.expires_at) < new Date()) {
    return notFound('Share has expired');
  }

  // Fetch from R2
  const object = await c.env.SHARE_BUCKET.get(`share_assets/${shareId}.bin`);
  if (!object) {
    return notFound('Share bundle not found in storage');
  }

  // Increment view count (fire-and-forget -- non-critical)
  meta.view_count += 1;
  const remainingTtl = Math.max(
    60,
    Math.floor((new Date(meta.expires_at).getTime() - Date.now()) / 1000) + 86400
  );
  c.executionCtx.waitUntil(
    c.env.SHARE_KV.put(`share:${shareId}`, JSON.stringify(meta), {
      expirationTtl: remainingTtl,
    })
  );

  return new Response(object.body, {
    status: 200,
    headers: {
      'Content-Type': 'application/octet-stream',
      'Content-Length': meta.size_bytes.toString(),
      'Cache-Control': 'private, no-cache',
    },
  });
}
