import init, * as bindings from './chat-sidebar-4bec4b9197bed6e3.js';
const wasm = await init({ module_or_path: './chat-sidebar-4bec4b9197bed6e3_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));