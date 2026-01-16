// Auto-generated init.js for Chat Sidebar WASM
// This file is loaded to initialize the WASM module

// Get the base URL for this script's directory
const scriptUrl = import.meta.url;
const baseUrl = scriptUrl.substring(0, scriptUrl.lastIndexOf('/') + 1);

// Import and initialize WASM with correct paths
const wasmJsUrl = baseUrl + 'chat-sidebar-eafcdd8ba4f444eb.js';
const wasmBinaryUrl = baseUrl + 'chat-sidebar-eafcdd8ba4f444eb_bg.wasm';

const module = await import(wasmJsUrl);
const wasm = await module.default({ module_or_path: wasmBinaryUrl });

window.wasmBindings = module;

dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));
