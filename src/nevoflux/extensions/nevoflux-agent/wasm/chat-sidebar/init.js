import init, * as bindings from './chat-sidebar-763e695b45651c06.js';
const wasm = await init({ module_or_path: './chat-sidebar-763e695b45651c06_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));