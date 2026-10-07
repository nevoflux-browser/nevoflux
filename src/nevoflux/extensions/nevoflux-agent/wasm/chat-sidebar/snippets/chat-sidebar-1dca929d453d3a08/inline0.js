
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

export function initCodeCopyDelegation() {
    if (window.__codeCopyInit) return;
    window.__codeCopyInit = true;
    document.addEventListener('click', function(e) {
        const btn = e.target.closest('.markdown-content .code-copy-btn');
        if (!btn) return;
        const codeBlock = btn.closest('.code-block');
        if (!codeBlock) return;
        const pre = codeBlock.querySelector('pre');
        if (!pre) return;
        const text = pre.textContent;
        // execCommand fallback (works in extension sidebar)
        try {
            const ta = document.createElement('textarea');
            ta.value = text;
            ta.style.cssText = 'position:fixed;left:-9999px';
            document.body.appendChild(ta);
            ta.select();
            const ok = document.execCommand('copy');
            document.body.removeChild(ta);
            if (ok) {
                btn.textContent = 'Copied!';
                btn.classList.add('copied');
                setTimeout(function() {
                    btn.textContent = 'Copy';
                    btn.classList.remove('copied');
                }, 2000);
                return;
            }
        } catch (_) {}
        // Async Clipboard API fallback
        navigator.clipboard.writeText(text).then(function() {
            btn.textContent = 'Copied!';
            btn.classList.add('copied');
            setTimeout(function() {
                btn.textContent = 'Copy';
                btn.classList.remove('copied');
            }, 2000);
        });
    });
}
