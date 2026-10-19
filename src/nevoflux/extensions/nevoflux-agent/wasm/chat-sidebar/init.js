import init, * as bindings from './chat-sidebar-47e16c405ac92d3d.js';
const wasm = await init({ module_or_path: './chat-sidebar-47e16c405ac92d3d_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));