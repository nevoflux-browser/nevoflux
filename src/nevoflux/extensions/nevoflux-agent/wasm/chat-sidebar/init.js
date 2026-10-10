import init, * as bindings from './chat-sidebar-bbd9c9c657f0c29c.js';
const wasm = await init({ module_or_path: './chat-sidebar-bbd9c9c657f0c29c_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));