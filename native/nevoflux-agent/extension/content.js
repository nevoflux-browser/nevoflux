// NevoFlux Agent - Content Script
// Injected into web pages to enable browser interaction

console.log('[NevoFlux] Content script loaded');

/**
 * Send a message to the background service worker
 * @param {Object} message - Message to send
 * @returns {Promise<Object>} Response from background
 */
async function sendToBackground(message) {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendMessage(message, (response) => {
      if (chrome.runtime.lastError) {
        reject(new Error(chrome.runtime.lastError.message));
      } else {
        resolve(response);
      }
    });
  });
}

/**
 * Listen for messages from the background service worker
 */
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.source !== 'background') {
    return;
  }

  console.log('[NevoFlux] Received from background:', message);

  switch (message.type) {
    case 'native-message':
      handleNativeMessage(message.data);
      break;

    case 'native-disconnected':
      console.log('[NevoFlux] Native host disconnected');
      break;

    default:
      break;
  }
});

/**
 * Handle messages from the native host (via background)
 * @param {Object} data - Message data from native host
 */
function handleNativeMessage(data) {
  console.log('[NevoFlux] Native message:', data);

  // Handle different message types from the native host
  if (data.type === 'command' && data.action) {
    executeCommand(data.action, data.payload);
  }
}

/**
 * Execute a command from the native host
 * @param {string} action - Action to execute
 * @param {Object} payload - Action payload
 */
function executeCommand(action, payload) {
  console.log('[NevoFlux] Executing command:', action, payload);

  switch (action) {
    case 'scroll':
      window.scrollBy(payload.x || 0, payload.y || 0);
      break;

    case 'get-page-info':
      const info = {
        url: window.location.href,
        title: document.title,
        scrollX: window.scrollX,
        scrollY: window.scrollY,
        width: window.innerWidth,
        height: window.innerHeight
      };
      sendToBackground({ type: 'send', payload: { type: 'response', data: info } });
      break;

    case 'get-element':
      try {
        // Validate selector is provided
        if (!payload.selector || typeof payload.selector !== 'string') {
          sendToBackground({
            type: 'send',
            payload: { type: 'response', data: { found: false, error: 'Invalid selector' } }
          });
          break;
        }

        const element = document.querySelector(payload.selector);
        if (element) {
          const rect = element.getBoundingClientRect();
          const text = element.textContent?.slice(0, 1000);
          sendToBackground({
            type: 'send',
            payload: {
              type: 'response',
              data: {
                found: true,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                text: text,
                truncated: element.textContent && element.textContent.length > 1000
              }
            }
          });
        } else {
          sendToBackground({
            type: 'send',
            payload: { type: 'response', data: { found: false } }
          });
        }
      } catch (e) {
        console.error('[NevoFlux] Invalid selector:', e.message);
        sendToBackground({
          type: 'send',
          payload: { type: 'response', data: { found: false, error: 'Invalid CSS selector' } }
        });
      }
      break;

    default:
      console.warn('[NevoFlux] Unknown command:', action);
  }
}
