import init, * as bindings from './chat-sidebar-5187d7dc6081fee.js';
const wasm = await init({ module_or_path: './chat-sidebar-5187d7dc6081fee_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));