#!/usr/bin/env bash
# Publish a complete rebuild without replacing previously released assets.
set -euo pipefail
: "${GITHUB_REPOSITORY:?}" "${GITHUB_RUN_ID:?}" "${GITHUB_RUN_ATTEMPT:?}" "${GITHUB_SHA:?}" "${RUNNER_TEMP:?}" "${GITHUB_STEP_SUMMARY:?}"
root=$1
targets=$2
run_id="$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"
[[ "$run_id" =~ ^[0-9]+-[0-9]+$ ]]
test "$GITHUB_REPOSITORY" = 'WSLd-wsx/wsld'
test "$(gh api "repos/$GITHUB_REPOSITORY" --jq .full_name)" = 'WSLd-wsx/wsld'
merged="$RUNNER_TEMP/wsx-rebuild-release-assets"
readback="$RUNNER_TEMP/wsx-rebuild-readback"
python3 scripts/merge-kernel-rebuild-assets.py "$root" "$targets" "$merged" "$run_id"
while IFS=$'\t' read -r target version; do
  [ -n "$version" ] || continue
  (cd "$root/$version" && sha256sum --check "SHA256SUMS-$version-$run_id")
done < "$targets"
tag="wsx-kernel-$run_id"
gh release create "$tag" --repo "$GITHUB_REPOSITORY" --target "$GITHUB_SHA" --draft --latest=false --title "WSX kernel modules $run_id" --notes 'Modules with source and build evidence. Runtime acceptance and source completeness require separate review.'
gh release upload "$tag" "$merged"/* --repo "$GITHUB_REPOSITORY"
mkdir "$readback"
for asset in "$merged"/*; do
  name=$(basename "$asset")
  gh release download "$tag" --repo "$GITHUB_REPOSITORY" --pattern "$name" --dir "$readback"
  cmp -s "$asset" "$readback/$name" || { echo "Release asset readback mismatch: $name" >&2; exit 1; }
done
while IFS=$'\t' read -r target version; do
  [ -n "$version" ] || continue
  (cd "$readback" && sha256sum --check "SHA256SUMS-$version-$run_id")
done < "$targets"
test "$(gh api "repos/$GITHUB_REPOSITORY" --jq .full_name)" = 'WSLd-wsx/wsld'
gh release edit "$tag" --repo "$GITHUB_REPOSITORY" --draft=false --latest=false
test "$(gh release view "$tag" --repo "$GITHUB_REPOSITORY" --json isDraft,isPrerelease,tagName --jq '(.isDraft == false and .isPrerelease == false) and (.tagName == "'"$tag"'")')" = true
echo "Verified candidate release: $tag. The existing wsx-kernel-modules release has not been changed." >> "$GITHUB_STEP_SUMMARY"
