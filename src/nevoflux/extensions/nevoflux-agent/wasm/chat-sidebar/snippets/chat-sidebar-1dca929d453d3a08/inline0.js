
export function base64_to_arraybuffer(base64_data) {
    const bin = atob(base64_data);
    const buf = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
    return buf.buffer;
}
