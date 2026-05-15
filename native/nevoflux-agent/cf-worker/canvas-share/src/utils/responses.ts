/**
 * Standard JSON response helpers for the Canvas Share API.
 */

/** Return a JSON success response. */
export function jsonOk<T>(data: T, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

/** Return a JSON error response. */
export function jsonError(message: string, status = 400): Response {
  return new Response(JSON.stringify({ error: message }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

/** Return a 404 not found response. */
export function notFound(message = 'Not found'): Response {
  return jsonError(message, 404);
}

/** Return a 403 forbidden response. */
export function forbidden(message = 'Forbidden'): Response {
  return jsonError(message, 403);
}

/** Return a 429 rate limited response. */
export function rateLimited(message = 'Rate limit exceeded'): Response {
  return jsonError(message, 429);
}

/** Return a 413 payload too large response. */
export function payloadTooLarge(message = 'Payload too large'): Response {
  return jsonError(message, 413);
}
