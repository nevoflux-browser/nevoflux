import type { Context } from 'hono';
import type { Env, ShareKVMeta } from '../types';
import { isValidShareId } from '../utils/validation';

/**
 * GET /c/:id
 *
 * Browser landing page for a shared canvas.
 * Shows share info and provides a button/link to import into NevoFlux.
 *
 * If NevoFlux is installed, clicking "Import" navigates to nevoflux://import/{share_id}
 * which triggers the protocol handler and starts the import flow.
 */
export async function handleLanding(c: Context<{ Bindings: Env }>): Promise<Response> {
  const shareId = c.req.param('id');

  if (!shareId || !isValidShareId(shareId)) {
    return new Response(renderErrorPage('Invalid Share Link', 'The share ID in this URL is not valid.'), {
      status: 400,
      headers: { 'Content-Type': 'text/html; charset=utf-8' },
    });
  }

  // Check if share exists
  const metaStr = await c.env.SHARE_KV.get(`share:${shareId}`);

  if (!metaStr) {
    return new Response(renderErrorPage('Share Not Found', 'This share may have expired or been deleted.'), {
      status: 404,
      headers: { 'Content-Type': 'text/html; charset=utf-8' },
    });
  }

  const meta: ShareKVMeta = JSON.parse(metaStr);

  if (new Date(meta.expires_at) < new Date()) {
    return new Response(renderErrorPage('Share Expired', 'This share has expired and is no longer available.'), {
      status: 410,
      headers: { 'Content-Type': 'text/html; charset=utf-8' },
    });
  }

  const html = renderSharePage(shareId, meta);

  return new Response(html, {
    status: 200,
    headers: { 'Content-Type': 'text/html; charset=utf-8' },
  });
}

function renderSharePage(shareId: string, meta: ShareKVMeta): string {
  const sizeKB = Math.round(meta.size_bytes / 1024);
  const expiresDate = new Date(meta.expires_at).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>NevoFlux Canvas Share</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: system-ui, -apple-system, sans-serif;
      background: #0f0f14;
      color: #e0e0e8;
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .card {
      background: #1a1a24;
      border: 1px solid #2a2a3a;
      border-radius: 16px;
      padding: 48px;
      max-width: 480px;
      width: 90%;
      text-align: center;
    }
    .logo { font-size: 32px; margin-bottom: 8px; }
    h1 { font-size: 24px; margin-bottom: 8px; color: #fff; }
    .subtitle { color: #888; margin-bottom: 32px; font-size: 14px; }
    .info {
      background: #12121a;
      border-radius: 8px;
      padding: 16px;
      margin-bottom: 24px;
      text-align: left;
    }
    .info-row {
      display: flex;
      justify-content: space-between;
      padding: 6px 0;
      font-size: 14px;
    }
    .info-label { color: #888; }
    .info-value { color: #ccc; }
    .import-btn {
      display: inline-block;
      background: #6366f1;
      color: #fff;
      padding: 14px 32px;
      border-radius: 10px;
      text-decoration: none;
      font-size: 16px;
      font-weight: 600;
      transition: background 0.15s;
      margin-bottom: 16px;
    }
    .import-btn:hover { background: #4f46e5; }
    .note {
      color: #666;
      font-size: 12px;
      line-height: 1.5;
    }
    .note a { color: #6366f1; text-decoration: none; }
    .note a:hover { text-decoration: underline; }
  </style>
</head>
<body>
  <div class="card">
    <div class="logo">&#x2728;</div>
    <h1>Shared Canvas</h1>
    <p class="subtitle">Someone shared a NevoFlux canvas with you</p>
    <div class="info">
      <div class="info-row">
        <span class="info-label">Size</span>
        <span class="info-value">${sizeKB} KB</span>
      </div>
      <div class="info-row">
        <span class="info-label">Views</span>
        <span class="info-value">${meta.view_count}</span>
      </div>
      <div class="info-row">
        <span class="info-label">Expires</span>
        <span class="info-value">${expiresDate}</span>
      </div>
    </div>
    <a href="nevoflux://import/${shareId}" class="import-btn">
      Import in NevoFlux
    </a>
    <p class="note">
      You will need the share password to decrypt this canvas.<br>
      Don't have NevoFlux? <a href="https://nevoflux.app/download">Download it here</a>.
    </p>
  </div>
</body>
</html>`;
}

function renderErrorPage(title: string, message: string): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${title} - NevoFlux</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: system-ui, -apple-system, sans-serif;
      background: #0f0f14;
      color: #e0e0e8;
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .card {
      background: #1a1a24;
      border: 1px solid #2a2a3a;
      border-radius: 16px;
      padding: 48px;
      max-width: 480px;
      width: 90%;
      text-align: center;
    }
    h1 { font-size: 24px; margin-bottom: 16px; color: #f87171; }
    p { color: #888; font-size: 14px; }
  </style>
</head>
<body>
  <div class="card">
    <h1>${title}</h1>
    <p>${message}</p>
  </div>
</body>
</html>`;
}
