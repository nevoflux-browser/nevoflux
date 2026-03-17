
export function copyTextFallback(text) {
    // execCommand fallback — works in extension sidebar where
    // navigator.clipboard.writeText is blocked by security context
    try {
        const ta = document.createElement('textarea');
        ta.value = text;
        ta.style.cssText = 'position:fixed;left:-9999px';
        document.body.appendChild(ta);
        ta.select();
        const ok = document.execCommand('copy');
        document.body.removeChild(ta);
        if (ok) return true;
    } catch (_) {}
    return false;
}
