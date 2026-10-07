import init, * as bindings from './chat-sidebar-230c5168c517f3b0.js';
const wasm = await init({ module_or_path: './chat-sidebar-230c5168c517f3b0_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));