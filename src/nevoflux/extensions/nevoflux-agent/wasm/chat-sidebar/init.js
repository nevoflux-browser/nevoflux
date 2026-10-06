import init, * as bindings from './chat-sidebar-9609c37a67900790.js';
const wasm = await init({ module_or_path: './chat-sidebar-9609c37a67900790_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));