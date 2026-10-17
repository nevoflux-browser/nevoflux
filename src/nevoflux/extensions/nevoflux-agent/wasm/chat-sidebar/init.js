import init, * as bindings from './chat-sidebar-d8d0074a93b7b335.js';
const wasm = await init({ module_or_path: './chat-sidebar-d8d0074a93b7b335_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));