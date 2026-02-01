#!/usr/bin/env python3
import re
import os
import sys
import time

def fix_csp(dist_dir):
    html_file = os.path.join(dist_dir, 'index.html')
    init_js = os.path.join(dist_dir, 'init.js')

    if not os.path.exists(html_file):
        print(f"Error: {html_file} not found")
        sys.exit(1)

    print(f"Fixing CSP for {html_file}...")

    with open(html_file, 'r') as f:
        content = f.read()

    # Find the inline script
    pattern = r'<script type=module>(.*?)</script>'
    match = re.search(pattern, content, re.DOTALL)

    timestamp = int(time.time())

    if match:
        script_content = match.group(1)
        with open(init_js, 'w') as f:
            f.write(script_content)
        print(f"Extracted inline script to {init_js}")

        new_content = re.sub(
            pattern,
            f'<script type="module" src="init.js?v={timestamp}"></script>',
            content,
            flags=re.DOTALL
        )
    else:
        # Try to find existing init.js and update timestamp
        pattern_ext = r'<script type="module" src="init.js(?:\?v=\d+)?"></script>'
        if re.search(pattern_ext, content):
            new_content = re.sub(
                pattern_ext,
                f'<script type="module" src="init.js?v={timestamp}"></script>',
                content
            )
            print(f"Updated existing script tag with timestamp v={timestamp}")
        else:
            print("No suitable script tag found to modify.")
            return

    # Fix relative paths in HTML
    new_content = new_content.replace('href=/', 'href=./')
    new_content = new_content.replace('src=/', 'src=./')

    # Remove legacy bridge.js script tag if present (no longer needed - WASM calls browser.nevoflux.* directly via js_sys)
    bridge_script = '<script src="bridge.js"></script>'
    if bridge_script in new_content:
        new_content = new_content.replace(bridge_script, '')
        print("Removed legacy bridge.js script tag (WASM now calls browser.nevoflux.* directly)")

    with open(html_file, 'w') as f:
        f.write(new_content)

    # Fix paths in init.js
    if os.path.exists(init_js):
        with open(init_js, 'r') as f:
            js_content = f.read()
        
        # Simple string replacements for common trunk patterns
        js_content = js_content.replace("from '/", "from './")
        js_content = js_content.replace('from "/', 'from "./')
        js_content = js_content.replace("module_or_path: '/", "module_or_path: './")
        js_content = js_content.replace('module_or_path: "/', 'module_or_path: "./')

        with open(init_js, 'w') as f:
            f.write(js_content)
        print(f"Fixed paths in {init_js} to be relative")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: fix-csp.py <dist_dir>")
        sys.exit(1)
    fix_csp(sys.argv[1])
