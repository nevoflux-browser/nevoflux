# NevoFlux Canvas Share — Cloudflare Worker

Backend API for NevoFlux Canvas Share. Stores end-to-end encrypted `NFEB` canvas bundles in R2 and their metadata in KV, behind a small Hono router.

The server never sees plaintext canvas data or the share password: the client encrypts locally with a password-derived key and uploads the opaque NFEB blob here.

## Endpoints

| Method | Path                        | Purpose                                          |
| ------ | --------------------------- | ------------------------------------------------ |
| POST   | `/api/share`                | Upload encrypted NFEB bundle (query-param auth)  |
| GET    | `/api/share/:id/bundle`     | Download encrypted bundle                        |
| GET    | `/api/share/:id/meta`       | Fetch metadata only (size, expiry, view count)   |
| PATCH  | `/api/share/:id`            | Extend TTL (requires `owner_token`)              |
| DELETE | `/api/share/:id`            | Delete share (requires `owner_token`)            |
| GET    | `/c/:id`                    | HTML landing page with `nevoflux://import/{id}`  |
| GET    | `/health`                   | Health check                                     |

A cron trigger (every 6h) sweeps KV/R2 for expired entries.

## Setup

Install deps:

```bash
npm install
```

Create the storage bindings:

```bash
# R2 bucket
npx wrangler r2 bucket create nevoflux-canvas-share

# KV namespace — copy the printed id into wrangler.toml
npx wrangler kv namespace create SHARE_KV
```

Edit `wrangler.toml` and replace `placeholder-kv-id` with the namespace id printed by the command above.

## Commands

```bash
npm run dev         # local worker on http://127.0.0.1:8787 (Miniflare)
npm run test        # vitest unit tests (validation helpers)
npm run typecheck   # tsc --noEmit
npm run deploy      # wrangler deploy
```

## Rate limiting

Rate limits are **not** implemented in code. Configure them in the Cloudflare Dashboard (Security > WAF > Rate limiting rules). Intended rules are documented inline in `wrangler.toml`.

## Layout

```
src/
  index.ts              Hono router, CORS, scheduled handler
  types.ts              Env + request/response types
  handlers/             one file per endpoint
  utils/
    responses.ts        jsonOk / jsonError / notFound / forbidden / ...
    validation.ts       isValidShareId / isValidOwnerTokenHash / verifyOwnerToken
test/
  index.test.ts         validation unit tests
```
