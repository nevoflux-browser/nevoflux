import init, * as bindings from './chat-sidebar-3961e22b90436fb1.js';
const wasm = await init({ module_or_path: './chat-sidebar-3961e22b90436fb1_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));