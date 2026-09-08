import init, * as bindings from './chat-sidebar-b8ae974db75357b8.js';
const wasm = await init({ module_or_path: './chat-sidebar-b8ae974db75357b8_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));