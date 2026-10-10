import init, * as bindings from './chat-sidebar-f70f821d847900ef.js';
const wasm = await init({ module_or_path: './chat-sidebar-f70f821d847900ef_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));