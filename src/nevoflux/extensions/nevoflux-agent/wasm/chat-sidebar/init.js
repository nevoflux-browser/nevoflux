import init, * as bindings from './chat-sidebar-dc96d4f746b8108.js';
const wasm = await init({ module_or_path: './chat-sidebar-dc96d4f746b8108_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));