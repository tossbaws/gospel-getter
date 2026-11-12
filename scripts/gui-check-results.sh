# shellcheck shell=bash
# results.txt's verdicts, for scripts/gui-check.sh, which sources this (and
# tests/gui's report tests, without a display). One line per check, by
# number and name: PASS (it ran and passed), SKIP (it can't run on this
# platform, as 26, 45 and 65 on Windows: neither a pass nor a failure) or
# FAIL. Each function reads the caller's $results (the file) and $checks
# (every check, "N name", as `gui-check --list` prints them).
# shellcheck disable=SC2154

# Every check gets a line in results.txt: those with no result (no PASS,
# SKIP or FAIL line) get a FAIL saying why ($1).
complete_results() {
    local check
    for check in "${checks[@]}"; do
        grep -qE "^(PASS|SKIP|FAIL) ${check%% *} " "$results" || echo "FAIL $check: $1" >>"$results"
    done
}

# results.txt's totals, each verdict on its own: "N passed, N skipped, N
# failed".
result_totals() {
    local passed skipped failed
    passed="$(grep -c '^PASS ' "$results" || true)"
    skipped="$(grep -c '^SKIP ' "$results" || true)"
    failed="$(grep -c '^FAIL ' "$results" || true)"
    echo "$passed passed, $skipped skipped, $failed failed"
}

# The run's exit status, from the checks' own ($1): a FAIL in results.txt
# fails a run that would otherwise pass; a SKIP never does.
run_status() {
    local status=$1
    if ((status == 0)) && grep -q '^FAIL ' "$results"; then
        status=1
    fi
    echo "$status"
}
