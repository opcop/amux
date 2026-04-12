#!/usr/bin/env bash
# Pre-commit gate: run `cargo test --workspace` before every `git commit`.
#
# Wired to Claude Code via .claude/settings.json as a PreToolUse hook on Bash.
# Claude Code sends the tool invocation as JSON on stdin:
#   { "tool_name": "Bash", "tool_input": { "command": "..." } }
#
# This hook only runs cargo test for commands that start with `git commit`.
# All other bash commands pass through untouched.
#
# Exit codes (Claude Code convention):
#   0 — allow the tool call
#   2 — block the tool call and feed stderr back to Claude

set -uo pipefail

input=$(cat)

# Pull tool_input.command out of the JSON payload.
# If jq isn't available, fall back to a grep that looks for `git commit` anywhere.
if command -v jq >/dev/null 2>&1; then
    cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
else
    cmd=$(printf '%s' "$input")
fi

case "$cmd" in
    "git commit"*|*"&& git commit"*|*"; git commit"*)
        ;;
    *)
        exit 0
        ;;
esac

cd "${CLAUDE_PROJECT_DIR:-$PWD}" || exit 0

printf 'pre-commit hook: running cargo test --workspace\n' >&2
if ! cargo test --workspace --quiet 2>&1 | tail -40 >&2; then
    printf '\npre-commit hook: cargo test failed — commit blocked.\n' >&2
    printf 'fix the failing tests, then retry the commit.\n' >&2
    exit 2
fi

exit 0
