#!/usr/bin/env bash
# Test cross-compile patches for rules.mk
# Verifies that apply-patches.sh correctly patches C/C++ and .res rules
# Usage: bash scripts/test-cross-compile-patches.sh [--with-make]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
ENGINE_RULES_MK="${ROOT_DIR}/engine/config/rules.mk"

PASS=0
FAIL=0
WITH_MAKE=false

for arg in "$@"; do
  [[ "$arg" == "--with-make" ]] && WITH_MAKE=true
done

pass() {
  PASS=$((PASS + 1))
  echo "  PASS: $1"
}
fail() {
  FAIL=$((FAIL + 1))
  echo "  FAIL: $1"
}

check() {
  local desc="$1" expected="$2" actual="$3"
  if [[ "$actual" == "$expected" ]]; then
    pass "$desc"
  else
    fail "$desc"
    echo "    expected: $expected"
    echo "    actual:   $actual"
  fi
}

# ============================================================
# Layer 1: Sed Pattern Unit Tests
# ============================================================
echo "=== Layer 1: Sed Pattern Unit Tests ==="

# --- C/C++ rules: should be patched ---
# These lines end with $< and contain COMPILE_C* — sed should wrap them
c_lines=(
  '	$(CC) $(OUTOPTION)$@ -c $(COMPILE_CFLAGS) $($(notdir $<)_FLAGS) $<'
  '	$(CCC) $(OUTOPTION)$@ -c $(COMPILE_CXXFLAGS) $($(notdir $<)_FLAGS) $<'
  '	$(CCC) -o $@ -c $(COMPILE_CXXFLAGS) $(COMPILE_CMMFLAGS) $($(notdir $<)_FLAGS) $<'
  '	$(CC) -o $@ -c $(COMPILE_CFLAGS) $(COMPILE_CMFLAGS) $($(notdir $<)_FLAGS) $<'
  '	$(CCC) -S $(COMPILE_CXXFLAGS) $($(notdir $<)_FLAGS) $<'
  '	$(CC) -S $(COMPILE_CFLAGS) $($(notdir $<)_FLAGS) $<'
)

for line in "${c_lines[@]}"; do
  expected="${line/%\$</\$(call relativize,\$<)}"
  actual="$(echo "$line" | sed '/COMPILE_C/s/\$<$/$(call relativize,$<)/')"
  check "C/C++ patch: ${line:0:60}..." "$expected" "$actual"
done

# --- .res rule: should be patched ---
res_line='	$(PYTHON3) $(MOZILLA_DIR)/config/create_res.py $(DEFINES) $(INCLUDES) -o $@ $<'
res_expected='	$(PYTHON3) $(MOZILLA_DIR)/config/create_res.py $(DEFINES) $(INCLUDES) -o $@ $(call relativize,$<)'
res_actual="$(echo "$res_line" | sed '/create_res\.py/s/\$<$/$(call relativize,$<)/')"
check ".res patch: create_res.py line" "$res_expected" "$res_actual"

# --- Negative tests: lines that should NOT be modified ---
neg_lines=(
  '	$(CC) $(OUTOPTION)$@ -c $(COMPILE_CFLAGS) $($(notdir $<)_FLAGS) $(call relativize,$<)'                            # already patched
  'COMPILE_CFLAGS += $(COMPILE_PDB_FLAG)'                                                                             # no trailing $<
  '$(eval $(call PREPROCESS_RULES,c,CSRCS,CC,COMPILE_CFLAGS))'                                                        # eval line, no trailing $<
  '	$(call WINEWRAP,$(AS)) $(ASOUTOPTION)$@ $(ASFLAGS) $($(notdir $<)_FLAGS) $(AS_DASH_C_FLAG) $(call relativize,$<)' # assembly, already has relativize
)

for line in "${neg_lines[@]}"; do
  actual="$(echo "$line" | sed '/COMPILE_C/s/\$<$/$(call relativize,$<)/')"
  check "No-op: ${line:0:60}..." "$line" "$actual"
done

# --- Idempotency tests: applying sed twice should not double-wrap ---
echo ""
echo "--- Idempotency ---"
for line in "${c_lines[@]}"; do
  once="$(echo "$line" | sed '/COMPILE_C/s/\$<$/$(call relativize,$<)/')"
  twice="$(echo "$once" | sed '/COMPILE_C/s/\$<$/$(call relativize,$<)/')"
  check "Idempotent C/C++: ${line:0:50}..." "$once" "$twice"
done

res_once="$(echo "$res_line" | sed '/create_res\.py/s/\$<$/$(call relativize,$<)/')"
res_twice="$(echo "$res_once" | sed '/create_res\.py/s/\$<$/$(call relativize,$<)/')"
check "Idempotent .res: create_res.py line" "$res_once" "$res_twice"

# ============================================================
# Layer 2: Post-Import Verification
# ============================================================
echo ""
echo "=== Layer 2: Post-Import Verification ==="

if [ ! -f "$ENGINE_RULES_MK" ]; then
  echo "  SKIP: $ENGINE_RULES_MK not found (run 'npm run import' first)"
else
  # Check all COMPILE_C recipe lines use relativize
  # Recipe lines start with a tab and contain COMPILE_C
  bad_c_lines="$(grep -n $'\t.*COMPILE_C' "$ENGINE_RULES_MK" | grep '\$<$' | grep -v 'relativize' || true)"
  if [ -z "$bad_c_lines" ]; then
    pass "All COMPILE_C recipe lines use relativize"
  else
    fail "Unpatched COMPILE_C recipe lines found:"
    echo "$bad_c_lines" | while read -r l; do echo "    $l"; done
  fi

  # Check create_res.py recipe line uses relativize
  res_check="$(grep 'create_res\.py' "$ENGINE_RULES_MK" | grep '\$<' || true)"
  if echo "$res_check" | grep -q 'relativize'; then
    pass "create_res.py line uses relativize"
  elif [ -z "$res_check" ]; then
    pass "create_res.py line not found (may not be present in this build)"
  else
    fail "create_res.py line missing relativize: $res_check"
  fi

  # Check no double wrapping
  double_wrap="$(grep -c 'relativize.*relativize' "$ENGINE_RULES_MK" || true)"
  if [ "$double_wrap" -eq 0 ]; then
    pass "No double-wrapped relativize calls"
  else
    fail "Found $double_wrap lines with double relativize wrapping"
  fi

  # Check relativize function definition exists
  if grep -q '^relativize = ' "$ENGINE_RULES_MK"; then
    pass "relativize function definition exists"
  else
    fail "relativize function definition missing"
  fi

  # Count total relativize usage
  count="$(grep -c 'call relativize,' "$ENGINE_RULES_MK" || true)"
  echo "  INFO: \$(call relativize,\$<) appears $count times"
fi

# ============================================================
# Layer 3: Make Relativize Expansion Test (optional)
# ============================================================
if $WITH_MAKE; then
  echo ""
  echo "=== Layer 3: Make Relativize Expansion Test ==="

  TMPDIR="$(mktemp -d)"
  trap 'rm -rf "$TMPDIR"' EXIT

  cat > "$TMPDIR/Makefile" << 'MAKEFILE'
# Minimal test for the relativize function
topobjdir := /workspace/build/obj
empty :=
space := $(empty) $(empty)

ifdef WINE
relativize = $(if $(filter /%,$1),$(DEPTH)$(subst $(space),,$(foreach d,$(subst /, ,$(topobjdir)),/..))$1,$1)
else
relativize = $1
endif

DEPTH := .

.PHONY: test-abs test-rel test-noop

test-abs:
	@echo "$(call relativize,/workspace/src/foo.c)"

test-rel:
	@echo "$(call relativize,src/foo.c)"

test-noop:
	@echo "$(call relativize,/workspace/src/bar.c)"
MAKEFILE

  # Test with WINE=1 (cross-compile mode)
  abs_result="$(make -f "$TMPDIR/Makefile" WINE=1 test-abs 2>&1)"
  if [[ "$abs_result" == *"../"*"/workspace/src/foo.c" ]]; then
    pass "WINE=1: absolute path relativized"
  else
    fail "WINE=1: expected relative path, got: $abs_result"
  fi

  rel_result="$(make -f "$TMPDIR/Makefile" WINE=1 test-rel 2>&1)"
  check "WINE=1: relative path unchanged" "src/foo.c" "$rel_result"

  # Test without WINE (native build, no-op)
  noop_result="$(make -f "$TMPDIR/Makefile" test-noop 2>&1)"
  check "No WINE: absolute path passed through" "/workspace/src/bar.c" "$noop_result"
fi

# ============================================================
# Summary
# ============================================================
echo ""
echo "==============================="
echo "Results: $PASS passed, $FAIL failed"
echo "==============================="
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
