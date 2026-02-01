import init, * as bindings from './chat-sidebar-cc625d972dc9a3bf.js';
const wasm = await init({ module_or_path: './chat-sidebar-cc625d972dc9a3bf_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));