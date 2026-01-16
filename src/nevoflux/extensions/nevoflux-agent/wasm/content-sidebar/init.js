// Auto-generated init.js for Content Sidebar WASM
// This file is loaded by content-bootstrap.js to initialize the WASM module

// Get the base URL for this script's directory
const scriptUrl = import.meta.url;
const baseUrl = scriptUrl.substring(0, scriptUrl.lastIndexOf('/') + 1);

// Import and initialize WASM with correct paths
const wasmJsUrl = baseUrl + 'content-sidebar-5340ab8c28f5cabc.js';
const wasmBinaryUrl = baseUrl + 'content-sidebar-5340ab8c28f5cabc_bg.wasm';

const module = await import(wasmJsUrl);
const wasm = await module.default({ module_or_path: wasmBinaryUrl });

window.wasmBindings = module;

// Initialize the Content Sidebar agent
console.log('[NevoFlux] Calling init_content_sidebar...');
if (module.init_content_sidebar) {
  module.init_content_sidebar();
  console.log('[NevoFlux] init_content_sidebar called successfully');
} else {
  console.error('[NevoFlux] init_content_sidebar not found in module exports');
  console.log('[NevoFlux] Available exports:', Object.keys(module));
}

dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));
