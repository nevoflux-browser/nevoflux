// NevoFlux Agent - Background Service Worker
// Handles native messaging communication with the NevoFlux daemon

const NATIVE_HOST_NAME = 'com.nevoflux.agent';

let nativePort = null;
let isConnected = false;

/**
 * Connect to the native messaging host
 * @returns {boolean} Whether connection was successful
 */
function connectToNativeHost() {
  if (nativePort) {
    console.log('[NevoFlux] Already connected to native host');
    return true;
  }

  try {
    console.log('[NevoFlux] Connecting to native host:', NATIVE_HOST_NAME);
    nativePort = chrome.runtime.connectNative(NATIVE_HOST_NAME);

    nativePort.onMessage.addListener(handleNativeMessage);
    nativePort.onDisconnect.addListener(handleNativeDisconnect);

    isConnected = true;
    console.log('[NevoFlux] Connected to native host');
    return true;
  } catch (error) {
    console.error('[NevoFlux] Failed to connect to native host:', error);
    isConnected = false;
    return false;
  }
}

/**
 * Disconnect from the native messaging host
 */
function disconnectFromNativeHost() {
  if (nativePort) {
    nativePort.disconnect();
    nativePort = null;
    isConnected = false;
    console.log('[NevoFlux] Disconnected from native host');
  }
}

/**
 * Handle messages received from the native host
 * @param {Object} message - Message from native host
 */
function handleNativeMessage(message) {
  console.log('[NevoFlux] Received from native host:', message);

  // Broadcast to all extension contexts (popup, content scripts)
  chrome.runtime.sendMessage({
    source: 'background',
    type: 'native-message',
    data: message
  }).catch(() => {
    // Ignore errors if no listeners
  });
}

/**
 * Handle native host disconnection
 */
function handleNativeDisconnect() {
  const error = chrome.runtime.lastError;
  console.log('[NevoFlux] Native host disconnected');

  if (error) {
    console.error('[NevoFlux] Disconnect reason:', error.message);
  }

  nativePort = null;
  isConnected = false;

  // Notify extension contexts of disconnection
  chrome.runtime.sendMessage({
    source: 'background',
    type: 'native-disconnected',
    error: error?.message || null
  }).catch(() => {
    // Ignore errors if no listeners
  });
}

/**
 * Send a message to the native host
 * @param {Object} message - Message to send
 * @returns {boolean} Whether message was sent
 */
function sendToNativeHost(message) {
  if (!nativePort) {
    console.warn('[NevoFlux] Not connected to native host');
    return false;
  }

  try {
    console.log('[NevoFlux] Sending to native host:', message);
    nativePort.postMessage(message);
    return true;
  } catch (error) {
    console.error('[NevoFlux] Failed to send message:', error);
    return false;
  }
}

/**
 * Handle messages from popup and content scripts
 */
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  console.log('[NevoFlux] Received message:', message, 'from:', sender);

  switch (message.type) {
    case 'connect':
      const connected = connectToNativeHost();
      sendResponse({ success: connected, isConnected });
      break;

    case 'disconnect':
      disconnectFromNativeHost();
      sendResponse({ success: true, isConnected: false });
      break;

    case 'status':
      sendResponse({ isConnected });
      break;

    case 'send':
      if (!message.payload) {
        sendResponse({ success: false, error: 'No payload provided' });
        break;
      }
      const sent = sendToNativeHost(message.payload);
      sendResponse({ success: sent });
      break;

    case 'command':
      // Forward command to native host
      const commandSent = sendToNativeHost({
        type: 'command',
        action: message.action,
        payload: message.payload
      });
      sendResponse({ success: commandSent });
      break;

    default:
      console.warn('[NevoFlux] Unknown message type:', message.type);
      sendResponse({ success: false, error: 'Unknown message type' });
  }

  // Return true to indicate async response
  return true;
});

/**
 * Handle extension installation/update
 */
chrome.runtime.onInstalled.addListener((details) => {
  console.log('[NevoFlux] Extension installed/updated:', details.reason);

  if (details.reason === 'install') {
    console.log('[NevoFlux] First install - attempting native host connection');
    // Attempt initial connection
    connectToNativeHost();
  }
});

/**
 * Handle extension startup
 */
chrome.runtime.onStartup.addListener(() => {
  console.log('[NevoFlux] Extension started');
  // Attempt to reconnect on browser startup
  connectToNativeHost();
});

// Log service worker activation
console.log('[NevoFlux] Background service worker loaded');
