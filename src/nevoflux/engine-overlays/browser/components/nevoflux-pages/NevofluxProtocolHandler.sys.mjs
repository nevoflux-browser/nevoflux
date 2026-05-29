/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// No lazy imports needed — uses Services.io directly

/**
 * Built-in route table: nevoflux://{host} -> chrome://nevoflux/content/pages/{host}.html
 */
const BUILTIN_ROUTES = {
  home: 'chrome://nevoflux/content/pages/home.html',
  settings: 'chrome://nevoflux/content/pages/settings.html',
  canvas: 'chrome://nevoflux/content/pages/canvas.html',
  import: 'chrome://nevoflux/content/pages/canvas.html',
  history: 'chrome://nevoflux/content/pages/history.html',
  plan: 'chrome://nevoflux/content/pages/plan.html',
  render: 'chrome://nevoflux/content/pages/render.html',
  brain: 'chrome://nevoflux/content/pages/brain.html',
};

/**
 * Protocol handler for the nevoflux:// scheme.
 *
 * Routing logic:
 *   1. Extract host from nevoflux://{host}/{path...}
 *   2. If host matches BUILTIN_ROUTES -> redirect to chrome:// URL with query params
 *      - canvas: ?id={path[0]}&mode={path[1] || 'preview'}
 *      - settings: ?section={path[0] || 'general'}
 *      - plan: ?id={path[0]}
 *   3. If host === "x" -> dynamic route (future, NS_ERROR_NOT_IMPLEMENTED for now)
 *   4. Otherwise -> NS_ERROR_FILE_NOT_FOUND
 */
export class NevofluxProtocolHandler {
  scheme = 'nevoflux';

  allowPort(_port, _scheme) {
    return false;
  }

  newChannel(uri, loadInfo) {
    let parsedURL;
    try {
      parsedURL = new URL(uri.spec);
    } catch (e) {
      throw Components.Exception(`Invalid nevoflux URL: ${uri.spec}`, Cr.NS_ERROR_MALFORMED_URI);
    }

    const host = parsedURL.hostname;
    const pathSegments = parsedURL.pathname.replace(/^\/+/, '').split('/').filter(Boolean);

    // Dynamic route namespace (P2 -- not implemented yet)
    if (host === 'x') {
      throw Components.Exception('Dynamic routes not yet implemented', Cr.NS_ERROR_NOT_IMPLEMENTED);
    }

    const chromeBase = BUILTIN_ROUTES[host];
    if (!chromeBase) {
      throw Components.Exception(`Unknown nevoflux route: ${host}`, Cr.NS_ERROR_FILE_NOT_FOUND);
    }

    // Build query parameters based on host type
    const params = new URLSearchParams();

    switch (host) {
      case 'canvas':
        if (pathSegments[0]) {
          params.set('id', pathSegments[0]);
        }
        params.set('mode', pathSegments[1] || 'preview');
        break;

      case 'import':
        // nevoflux://import/{share_id}
        // -> canvas.html?share_id={share_id}&mode=import
        if (pathSegments[0]) {
          params.set('share_id', pathSegments[0]);
        }
        params.set('mode', 'import');
        break;

      case 'settings':
        params.set('section', pathSegments[0] || 'general');
        break;

      case 'plan':
        if (pathSegments[0]) {
          params.set('id', pathSegments[0]);
        }
        break;

      case 'render':
        // nevoflux://render/{job_id}/{composition_id?}
        // -> render.html?job_id={job_id}&composition_id={composition_id}
        if (pathSegments[0]) {
          params.set('job_id', pathSegments[0]);
        }
        if (pathSegments[1]) {
          params.set('composition_id', pathSegments[1]);
        }
        break;

      // home, history: no special params
    }

    // Preserve any explicit ?key=value the caller passed in — protocol-handler
    // path mappings take precedence, but extra params (e.g. debug flags) are
    // forwarded. Path-mapped keys win on collision.
    for (const [k, v] of parsedURL.searchParams) {
      if (!params.has(k)) {
        params.set(k, v);
      }
    }

    const queryString = params.toString();
    const targetSpec = queryString ? `${chromeBase}?${queryString}` : chromeBase;
    const targetURI = Services.io.newURI(targetSpec);

    // Create channel using the same pattern as AboutNewTabRedirector:
    // Pass the original loadInfo directly to preserve navigation context.
    const channel = Services.io.newChannelFromURIWithLoadInfo(targetURI, loadInfo);
    channel.originalURI = uri;
    return channel;
  }

  QueryInterface = ChromeUtils.generateQI(['nsIProtocolHandler']);
}
