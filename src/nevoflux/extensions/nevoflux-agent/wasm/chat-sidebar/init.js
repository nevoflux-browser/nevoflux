import init, * as bindings from './chat-sidebar-879703e418f5781.js';
const wasm = await init({ module_or_path: './chat-sidebar-879703e418f5781_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));