import init, * as bindings from './chat-sidebar-b19b96f192759d96.js';
const wasm = await init({ module_or_path: './chat-sidebar-b19b96f192759d96_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));