import init, * as bindings from './chat-sidebar-40b4689fa91bb473.js';
const wasm = await init({ module_or_path: './chat-sidebar-40b4689fa91bb473_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));