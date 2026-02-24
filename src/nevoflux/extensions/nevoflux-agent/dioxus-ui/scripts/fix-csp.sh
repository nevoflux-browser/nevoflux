#!/bin/bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Fix CSP issues by extracting inline scripts from Trunk-generated HTML
# This script is called as a post-build hook by Trunk

set -e

DIST_DIR="${1:-.}"
HTML_FILE="$DIST_DIR/index.html"
INIT_JS="$DIST_DIR/init.js"

if [ ! -f "$HTML_FILE" ]; then
  echo "Error: $HTML_FILE not found"
  exit 1
fi

echo "Fixing CSP for $HTML_FILE..."

# Use Python for reliable multiline extraction
export DIST_DIR
python3 << 'PYTHON_SCRIPT'
import re
import os
import time

dist_dir = os.environ.get('DIST_DIR', '.')
html_file = f"{dist_dir}/index.html"
init_js = f"{dist_dir}/init.js"

with open(html_file, 'r') as f:
    content = f.read()

# Find the inline script (handles multiline)
pattern = r'<script type=module>(.*?)</script>'
match = re.search(pattern, content, re.DOTALL)

if not match:
    # If already externalized, just update the timestamp
    pattern_ext = r'<script type="module" src="init.js(?:\?v=\d+)?"></script>'
    timestamp = int(time.time())
    new_content = re.sub(
        pattern_ext,
        f'<script type="module" src="init.js?v={timestamp}"></script>',
        content
    )
    with open(html_file, 'w') as f:
        f.write(new_content)
    print(f"Updated existing script tag with timestamp v={timestamp}")
    exit(0)

script_content = match.group(1)

# Write the script to init.js
with open(init_js, 'w') as f:
    f.write(script_content)
print(f"Extracted inline script to {init_js}")

# Replace inline script with external reference, adding a timestamp to bust cache
timestamp = int(time.time())
new_content = re.sub(
    pattern,
    f'<script type="module" src="init.js?v={timestamp}"></script>',
    content,
    flags=re.DOTALL
)

with open(html_file, 'w') as f:
    f.write(new_content)
print(f"Updated {html_file} to reference external init.js?v={timestamp}")

# Fix absolute paths in init.js to be relative (for extension context)
with open(init_js, 'r') as f:
    init_content = f.read()

# Change absolute paths like '/foo.js' to relative './foo.js'
init_content = re.sub(r'''from\s+(?:'|")/([\w-]+)''', r"from './\1", init_content)
init_content = re.sub(r'''module_or_path:\s+(?:'|")/([\w-]+)''', r"module_or_path: './\1", init_content)

with open(init_js, 'w') as f:
    f.write(init_content)
print(f"Fixed paths in {init_js} to be relative")

# Also fix HTML link/preload paths
with open(html_file, 'r') as f:
    html_content = f.read()

# Change href="/foo" to href="./foo" and src="/foo" to src="./foo"
html_content = re.sub(r'href=/', r'href=./', html_content)
html_content = re.sub(r'src=/', r'src=./', html_content)

with open(html_file, 'w') as f:
    f.write(html_content)
print(f"Fixed link paths in {html_file} to be relative")
PYTHON_SCRIPT

echo "CSP fix complete!"
