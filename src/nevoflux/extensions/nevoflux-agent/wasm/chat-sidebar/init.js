import init, * as bindings from './chat-sidebar-b3cb6ca626242163.js';
const wasm = await init({ module_or_path: './chat-sidebar-b3cb6ca626242163_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));