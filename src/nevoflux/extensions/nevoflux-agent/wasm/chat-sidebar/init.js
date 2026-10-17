import init, * as bindings from './chat-sidebar-612f7eb18a405983.js';
const wasm = await init({ module_or_path: './chat-sidebar-612f7eb18a405983_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));