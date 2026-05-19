#!/usr/bin/env bash
# Test scripts/gha/inject-agent.sh — arg validation + soul/skills placement
# without network. Mocks gh to a no-op.
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PASS=0
FAIL=0

assert_eq() {
  local what="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "PASS: $what"
    PASS=$((PASS+1))
  else
    echo "FAIL: $what (expected='$expected' actual='$actual')"
    FAIL=$((FAIL+1))
  fi
}

# Test 1: invalid arch → exit 1
set +e
(cd "$REPO_ROOT" && bash scripts/gha/inject-agent.sh invalid 2>/dev/null)
rc=$?
set -e
assert_eq "invalid-arch-rejected" "1" "$rc"

# Test 2: missing arg → exit 1
set +e
(cd "$REPO_ROOT" && bash scripts/gha/inject-agent.sh 2>/dev/null)
rc=$?
set -e
assert_eq "missing-arg-rejected" "1" "$rc"

# Test 3: x86_64 with mocked gh (no release) → exit 0
TMP=$(mktemp -d)
mkdir -p "$TMP/bin"
cat > "$TMP/bin/gh" <<'MOCK'
#!/usr/bin/env bash
# Mock: simulate "no releases" by returning exit 1 for `gh release view`
exit 1
MOCK
chmod +x "$TMP/bin/gh"

set +e
(
  cd "$REPO_ROOT"
  PATH="$TMP/bin:$PATH" \
  APPDIR_ROOT="$TMP/build" \
  AGENT_REPO="fake/repo" \
  bash scripts/gha/inject-agent.sh x86_64 >"$TMP/out.log" 2>&1
)
rc=$?
set -e
assert_eq "x86_64-no-release-exits-zero" "0" "$rc"

# Soul templates placement check (depends on docs/reference/templates existence)
if [ -d "$REPO_ROOT/docs/reference/templates" ] \
   && ls "$REPO_ROOT/docs/reference/templates"/*.md >/dev/null 2>&1; then
  if [ -d "$TMP/build/AppDir-x86_64/distribution/bin/defaults/soul" ] \
     && [ -n "$(ls "$TMP/build/AppDir-x86_64/distribution/bin/defaults/soul" 2>/dev/null)" ]; then
    assert_eq "x86_64-soul-placed" "yes" "yes"
  else
    assert_eq "x86_64-soul-placed" "yes" "no"
  fi
else
  echo "SKIP: x86_64-soul-placed (no docs/reference/templates/*.md in repo)"
fi

# Skills placement
if [ -d "$REPO_ROOT/docs/reference/skills" ]; then
  if [ -d "$TMP/build/AppDir-x86_64/distribution/bin/defaults/skills" ]; then
    assert_eq "x86_64-skills-placed" "yes" "yes"
  else
    assert_eq "x86_64-skills-placed" "yes" "no"
  fi
else
  echo "SKIP: x86_64-skills-placed (no docs/reference/skills in repo)"
fi

rm -rf "$TMP"

# Test 4: aarch64 separate dir
TMP=$(mktemp -d)
mkdir -p "$TMP/bin"
cat > "$TMP/bin/gh" <<'MOCK'
#!/usr/bin/env bash
exit 1
MOCK
chmod +x "$TMP/bin/gh"

set +e
(
  cd "$REPO_ROOT"
  PATH="$TMP/bin:$PATH" \
  APPDIR_ROOT="$TMP/build" \
  AGENT_REPO="fake/repo" \
  bash scripts/gha/inject-agent.sh aarch64 >"$TMP/out.log" 2>&1
)
rc=$?
set -e
assert_eq "aarch64-no-release-exits-zero" "0" "$rc"

if [ ! -d "$TMP/build/AppDir-x86_64" ]; then
  assert_eq "aarch64-no-cross-pollution" "yes" "yes"
else
  assert_eq "aarch64-no-cross-pollution" "yes" "no"
fi

rm -rf "$TMP"

echo
echo "Passed: $PASS"
echo "Failed: $FAIL"
[ "$FAIL" -eq 0 ]
