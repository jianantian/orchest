#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

targets=(
  "js"
  "examples"
  "docs/iteration/v0_1/issues"
)

patterns=(
  "runStarted"
  "modelCallStarted"
  "modelStreamChunk"
  "modelCallCompleted"
  "toolCallStarted"
  "toolCallUpdate"
  "toolCallCompleted"
  "toolCallFailed"
  "asyncToolStarted"
  "asyncToolProgress"
  "asyncToolCompleted"
  "skillContentRead"
  "approvalRequested"
  "approvalGranted"
  "approvalDenied"
  "budgetWarning"
  "runtimeWarning"
  "skillDependencyError"
  "skillMissingCapabilities"
  "contextCompacted"
  "subAgentStarted"
  "subAgentCompleted"
  "subAgentFailed"
  "childRunEvent"
  "runCompleted"
  "runFailed"
)

found=0
for pattern in "${patterns[@]}"; do
  if rg --fixed-strings --line-number --color never "$pattern" "${targets[@]}"; then
    found=1
  fi
done

if [[ "$found" -ne 0 ]]; then
  echo
  echo "error: camelCase RuntimeEvent wire discriminants are not allowed."
  echo "Use the canonical snake_case event.type values instead."
  exit 1
fi

echo "RuntimeEvent wire naming check passed."
