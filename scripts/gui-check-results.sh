# shellcheck shell=bash
# results.txt's verdicts, for scripts/gui-check.sh, which sources this (and
# tests/gui's report tests, without a display). One line per check, by
# number and name: PASS (it ran and passed), SKIP (it can't run on this
# platform, where `gui-check --list` says: neither a pass nor a failure),
# FAIL, or, after a retry (retry_suite), FLAKY (it failed in attempt 1 and
# passed in attempt 2: not a failure, and never counted as a pass). The
# result functions read the caller's $results (the file) and $checks (every
# check, "N name", the first two columns of `gui-check --list`);
# retry_suite reads $out and $runner.
# shellcheck disable=SC2154

# Every check gets a line in results.txt: those with no result (no PASS,
# SKIP, FAIL or FLAKY line) get a FAIL saying why ($1).
complete_results() {
    local check
    for check in "${checks[@]}"; do
        grep -qE "^(PASS|SKIP|FAIL|FLAKY) ${check%% *} " "$results" || echo "FAIL $check: $1" >>"$results"
    done
}

# results.txt's totals, each verdict on its own: "N passed, N skipped, N
# failed", and ", N flaky (passed on retry)" when a retry made any.
result_totals() {
    local passed skipped failed flaky
    passed="$(grep -c '^PASS ' "$results" || true)"
    skipped="$(grep -c '^SKIP ' "$results" || true)"
    failed="$(grep -c '^FAIL ' "$results" || true)"
    flaky="$(grep -c '^FLAKY ' "$results" || true)"
    if ((flaky > 0)); then
        echo "$passed passed, $skipped skipped, $failed failed, $flaky flaky (passed on retry)"
    else
        echo "$passed passed, $skipped skipped, $failed failed"
    fi
}

# The run's exit status, from the checks' own ($1): a FAIL in results.txt
# fails a run that would otherwise pass; a SKIP or a FLAKY never does.
run_status() {
    local status=$1
    if ((status == 0)) && grep -q '^FAIL ' "$results"; then
        status=1
    fi
    echo "$status"
}

# CI only (GUI_CHECK_RETRY=1, which dist.yml sets for both GUI jobs): runs
# the suite (the command "$@", given its out-dir after it) into $out, and
# if it fails, the whole suite once more, from the start as the first
# attempt did: attempt 1 is kept in $out/attempt-1 (its results.txt,
# screenshots and logs), fresh_app_data (the caller's) gives the app fresh
# data where a run can't (Windows), and attempt 2 runs into $out. Single
# checks are never retried in the first attempt's session: checks leave
# state behind, and a failure cascades. $runner (gui-check --retried) then
# judges both: attempt 2's verdicts, a check that failed in attempt 1 and
# passed in attempt 2 FLAKY, never a clean pass, with a ::warning each (on
# stdout) and retry-summary.md, added to the job's summary
# ($GITHUB_STEP_SUMMARY). The status is attempt 2's, and 0 only if that
# judgement passes too: a FAIL in attempt 2 fails the run.
retry_suite() {
    local status=0 judged=0 entry
    retry_pid=""
    trap 'retry_stop 130' INT
    trap 'retry_stop 143' TERM
    retry_attempt 1 "$@" || status=$?
    if ((status == 0 || status == 130 || status == 143)); then
        return "$status"
    fi
    echo "[gui-check retry] attempt 1 failed (exit $status): the whole suite once more, on fresh data" >&2
    if ! fresh_app_data; then
        echo "[gui-check retry] couldn't give the app fresh data, so no second attempt" >&2
        return "$status"
    fi
    mkdir "$out/attempt-1" || return "$status"
    for entry in "$out"/*; do
        [[ $entry == "$out/attempt-1" ]] || mv "$entry" "$out/attempt-1/" || return "$status"
    done
    status=0
    retry_attempt 2 "$@" || status=$?
    ((status == 130 || status == 143)) && return "$status"
    "$runner" --retried "$(retry_path "$out/attempt-1/results.txt")" \
        "$(retry_path "$out/results.txt")" "$(retry_path "$out/retry-summary.md")" || judged=$?
    if [[ -n ${GITHUB_STEP_SUMMARY:-} && -f $out/retry-summary.md ]]; then
        cat "$out/retry-summary.md" >>"$GITHUB_STEP_SUMMARY"
    fi
    echo "[gui-check retry] results after the retry (exit $status, judged $judged):" >&2
    cat "$out/results.txt" >&2
    ((status != 0)) && return "$status"
    return "$judged"
}

# One attempt of retry_suite: "$@" with its out-dir, as attempt $1
# (GUI_CHECK_ATTEMPT). In the background, waited for, so that a signal to
# this script stops it, and its own cleanup runs (TERM: a background
# command ignores INT).
retry_attempt() {
    local attempt=$1 status=0
    shift
    echo "[gui-check retry] attempt $attempt of 2 (GUI_CHECK_RETRY)" >&2
    GUI_CHECK_ATTEMPT=$attempt "$@" "$out" &
    retry_pid=$!
    wait "$retry_pid" || status=$?
    retry_pid=""
    return "$status"
}

# A signal during retry_suite: the attempt running is stopped, and its
# cleanup waited for, before this exits with $1.
retry_stop() {
    trap - INT TERM
    if [[ -n ${retry_pid:-} ]]; then
        kill -TERM "$retry_pid" 2>/dev/null
        wait "$retry_pid" 2>/dev/null
    fi
    exit "$1"
}

# A path as the checks binary takes it (Windows: its own form).
retry_path() {
    if command -v cygpath >/dev/null; then
        cygpath -w "$1"
    else
        printf '%s\n' "$1"
    fi
}
