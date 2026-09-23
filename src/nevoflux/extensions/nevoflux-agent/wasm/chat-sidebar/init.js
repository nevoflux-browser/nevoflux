import init, * as bindings from './chat-sidebar-8008167f52981fdf.js';
const wasm = await init({ module_or_path: './chat-sidebar-8008167f52981fdf_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));