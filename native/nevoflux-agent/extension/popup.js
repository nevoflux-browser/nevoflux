// NevoFlux Agent - Popup Script

const statusIndicator = document.getElementById('statusIndicator');
const statusText = document.getElementById('statusText');
const connectBtn = document.getElementById('connectBtn');
const statusBtn = document.getElementById('statusBtn');

let isConnected = false;

/**
 * Update the UI based on connection status
 * @param {boolean} connected - Whether connected to native host
 */
function updateUI(connected) {
  isConnected = connected;

  if (connected) {
    statusIndicator.classList.add('connected');
    statusText.textContent = 'Connected to agent';
    connectBtn.textContent = 'Disconnect';
  } else {
    statusIndicator.classList.remove('connected');
    statusText.textContent = 'Not connected';
    connectBtn.textContent = 'Connect';
  }
}

/**
 * Check current connection status
 */
async function checkStatus() {
  try {
    const response = await chrome.runtime.sendMessage({ type: 'status' });
    updateUI(response.isConnected);
  } catch (error) {
    console.error('Failed to check status:', error);
    updateUI(false);
  }
}

/**
 * Toggle connection to native host
 */
async function toggleConnection() {
  try {
    connectBtn.disabled = true;

    if (isConnected) {
      const response = await chrome.runtime.sendMessage({ type: 'disconnect' });
      updateUI(response.isConnected);
    } else {
      statusText.textContent = 'Connecting...';
      const response = await chrome.runtime.sendMessage({ type: 'connect' });
      updateUI(response.isConnected);

      if (!response.success) {
        statusText.textContent = 'Connection failed';
      }
    }
  } catch (error) {
    console.error('Failed to toggle connection:', error);
    statusText.textContent = 'Error: ' + error.message;
  } finally {
    connectBtn.disabled = false;
  }
}

// Event listeners
connectBtn.addEventListener('click', toggleConnection);
statusBtn.addEventListener('click', checkStatus);

// Listen for messages from background
chrome.runtime.onMessage.addListener((message) => {
  if (message.source === 'background') {
    if (message.type === 'native-disconnected') {
      updateUI(false);
      if (message.error) {
        statusText.textContent = 'Disconnected: ' + message.error;
      }
    }
  }
});

// Check status on popup open
checkStatus();
