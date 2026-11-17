#!/usr/bin/env bash
# Build and publish one target discovered by the remote workflow.
set -euo pipefail
: "${TARGET_TAG:?}" "${TARGET_VERSION:?}" "${TARGET_COMMIT:?}" "${RUNNER_TEMP:?}"
commit=$TARGET_COMMIT
python3 scripts/export-kernel-build-inputs.py --output "$RUNNER_TEMP/wsx-build-inputs.tar.gz"
mkdir -p "$RUNNER_TEMP/wsx-coverage"
python3 scripts/fetch-target-kernel-package.py --tag "$TARGET_TAG" --version "$TARGET_VERSION" --coverage "$RUNNER_TEMP/wsx-coverage"
EXPECTED_VERSION="$TARGET_VERSION" python3 -c 'import json,os,pathlib; p=pathlib.Path(os.environ["RUNNER_TEMP"])/"wsx-coverage/target-inputs"/(os.environ["TARGET_TAG"]+".json"); assert json.loads(p.read_text())["version"] == os.environ["EXPECTED_VERSION"]'
python3 scripts/build-coverage-target.py --tag "$TARGET_TAG" --commit "$commit" --coverage "$RUNNER_TEMP/wsx-coverage"

python3 -c 'import importlib.util,os,pathlib; p=pathlib.Path("scripts/kernel-staging-snapshots.py"); s=importlib.util.spec_from_file_location("staging",p); m=importlib.util.module_from_spec(s); s.loader.exec_module(m); root=pathlib.Path(os.environ["RUNNER_TEMP"]); assert m.extract_current(root/"wsx-coverage",root/"wsx-snapshot") is not None'
python3 scripts/export-kernel-release-assets.py --snapshot "$RUNNER_TEMP/wsx-snapshot" --build-inputs "$RUNNER_TEMP/wsx-build-inputs.tar.gz" --output "$RUNNER_TEMP/wsx-release-assets" | tee -a "$GITHUB_STEP_SUMMARY"
python3 -m unittest discover -s tests -p 'test_*kernel_release*.py' -v

test "$(gh api "repos/$GITHUB_REPOSITORY" --jq .full_name)" = 'WSLd-wsx/wsld'
mapfile -t captures < <(find "$RUNNER_TEMP/wsx-coverage/diagnostic-builds" -path "*/modules-build/build-result.json" -type f)
test "${#captures[@]}" -eq 1
workspace=$(dirname "$(dirname "${captures[0]}")")
python3 scripts/attach-kernel-source-evidence.py --assets "$RUNNER_TEMP/wsx-release-assets" --workspace "$workspace" --source-archive "$workspace/kernel-source.tar.gz" | tee -a "$GITHUB_STEP_SUMMARY"
python3 scripts/export-kernel-build-evidence.py --workspace "$workspace" --output "$RUNNER_TEMP/build-evidence.tar.gz" --descriptor "$RUNNER_TEMP/build-evidence.json"
python3 scripts/export-kernel-module-sources.py --root "$PWD" --dependencies "$WSX_KERNEL_RUNNER_DEPENDENCIES" --output "$RUNNER_TEMP/module-sources.tar.gz" --descriptor "$RUNNER_TEMP/module-sources.json"
python3 - <<'PYCODE'
import hashlib, json, os, shutil, tarfile
from pathlib import Path
root = Path(os.environ['RUNNER_TEMP'])
assets = root / 'wsx-release-assets'
descriptor = json.loads((root / 'module-sources.json').read_text())
shutil.copyfile(root / 'module-sources.tar.gz', assets / descriptor['asset'])
build_evidence = json.loads((root / 'build-evidence.json').read_text())
shutil.copyfile(root / 'build-evidence.tar.gz', assets / build_evidence['asset'])
manifest_path = assets / 'source-materials.json'
manifest = json.loads(manifest_path.read_text())
index = json.loads((assets / 'index.json').read_text())
with tarfile.open(assets / index['build_inputs']['asset'], 'r:gz') as inputs_tar:
    inputs_manifest = json.load(inputs_tar.extractfile('manifest.json'))
if inputs_manifest['generation'] != index['generation']:
    raise ValueError('Recorded build inputs differ from snapshot generation')
with tarfile.open(root / 'module-sources.tar.gz', 'r:gz') as sources_tar:
    source_manifest = json.load(sources_tar.extractfile('manifest.json'))
    members = sources_tar.getmembers()
    if len(members) != len(source_manifest['files']) + 1 or {x.name for x in members} != set(source_manifest['files']) | {'manifest.json'}:
        raise ValueError('Module source archive members differ from its manifest')
    for name, expected in source_manifest['files'].items():
        stream = sources_tar.extractfile(name)
        input_name = '@runner-dependencies' if name == 'runner-dependencies.json' else name.removeprefix('inputs/')
        if stream is None or hashlib.file_digest(stream, 'sha256').hexdigest() != expected or inputs_manifest['inputs'].get(input_name) != expected:
            raise ValueError('Module source archive differs from recorded build inputs: ' + name)
manifest['recorded_build_inputs'] = index['build_inputs']
manifest['snapshot_evidence'] = index['evidence']
manifest['build_evidence'] = build_evidence
manifest['build_inputs'] = descriptor
manifest['build_inputs_scope'] = 'module sources and selected build dependencies'
manifest_path.write_text(json.dumps(manifest, sort_keys=True, indent=2) + '\n')
PYCODE
python3 scripts/export-kernel-buildid-asset.py --snapshot "$RUNNER_TEMP/wsx-snapshot" --kernel-exports "$workspace/target-inspection/kernel-exports.json" --output "$RUNNER_TEMP/wsx-buildid-assets" | tee -a "$GITHUB_STEP_SUMMARY"
rm "$RUNNER_TEMP/wsx-release-assets"/modules-*.tar.gz "$RUNNER_TEMP/wsx-release-assets/index.json"
python3 - <<'PYCODE'
import json, os
from pathlib import Path
root = Path(os.environ['RUNNER_TEMP'])
assets = root / 'wsx-buildid-assets'
metadata_path, = assets.glob('modules-*.json')
metadata = json.loads(metadata_path.read_text())
target = json.loads((root / 'wsx-coverage/target-inputs' / (os.environ['TARGET_TAG'] + '.json')).read_text())
if target['version'] != os.environ['TARGET_VERSION']:
    raise ValueError('Unexpected target kernel version')
metadata['kernel_version'] = target['version']
archive = assets / metadata['archive']
renamed = assets / ('modules_' + target['version'] + '.tar.gz')
archive.rename(renamed)
metadata['archive'] = renamed.name
metadata_path.write_text(json.dumps(metadata, sort_keys=True, separators=(',', ':')) + '\n')
PYCODE
cp "$RUNNER_TEMP/wsx-buildid-assets"/* "$RUNNER_TEMP/wsx-release-assets/"
python3 - "$workspace" <<'PYCODE'
import hashlib, json, os, pathlib, sys, tarfile
p = pathlib.Path(os.environ['RUNNER_TEMP']) / 'wsx-release-assets'
f = p / 'source-materials.json'
d = json.loads(f.read_text())
d.pop('index_sha256'); d.pop('groups')
m = next(p.glob('modules-*.json')); v = json.loads(m.read_text())
build = json.loads((pathlib.Path(sys.argv[1]) / 'modules-build/build-result.json').read_text())
if build['kernel_source_commit'] != d['kernel_source_commit'] or build['build_generation'] != d['build_generation']:
    raise ValueError('Build result differs from source evidence')
archive = p / v['archive']
with tarfile.open(archive, 'r:gz') as tar:
    members = tar.getmembers()
    if len(members) != len(build['modules']) or {x.name for x in members} != set(build['modules']):
        raise ValueError('Module archive contents differ from build result')
    for item in members:
        stream = tar.extractfile(item)
        expected = build['modules'][item.name]
        if not item.isfile() or stream is None or item.size != expected['bytes'] or hashlib.file_digest(stream, 'sha256').hexdigest() != expected['sha256']:
            raise ValueError('Module archive differs from build result: ' + item.name)
d['kernel_build_id'] = v['kernel_build_id']
d['module_metadata_sha256'] = hashlib.sha256(m.read_bytes()).hexdigest()
d['module_archive'] = {'asset': v['archive'], 'sha256': v['archive_sha256'], 'bytes': v['archive_bytes']}
d['modules'] = build['modules']
f.write_text(json.dumps(d, sort_keys=True, indent=2) + '\n')
PYCODE
run_id="${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"
mv "$RUNNER_TEMP/wsx-release-assets/source-materials.json" "$RUNNER_TEMP/wsx-release-assets/source-materials-$TARGET_VERSION-$run_id.json"
metadata=$(find "$RUNNER_TEMP/wsx-release-assets" -maxdepth 1 -name 'modules-*.json' -print -quit)
mv "$metadata" "$RUNNER_TEMP/wsx-release-assets/modules_$TARGET_VERSION-$run_id.json"
report="$RUNNER_TEMP/wsx-release-assets/source-review-$TARGET_VERSION-$run_id.json"
review_args=()
if [[ -n "${WSX_KERNEL_SOURCE_REVIEW:-}" ]]; then
  review_args=(--review "$WSX_KERNEL_SOURCE_REVIEW")
fi
python3 scripts/review-kernel-source-materials.py --assets "$RUNNER_TEMP/wsx-release-assets" \
  --source-materials "$RUNNER_TEMP/wsx-release-assets/source-materials-$TARGET_VERSION-$run_id.json" \
  --metadata "$RUNNER_TEMP/wsx-release-assets/modules_$TARGET_VERSION-$run_id.json" \
  --output "$report" "${review_args[@]}" | tee -a "$GITHUB_STEP_SUMMARY"
python3 - "$RUNNER_TEMP/wsx-release-assets/source-materials-$TARGET_VERSION-$run_id.json" "$report" <<'PYCODE'
import hashlib, json, pathlib, sys
manifest_path, report_path = map(pathlib.Path, sys.argv[1:])
manifest = json.loads(manifest_path.read_text())
report = json.loads(report_path.read_text())
manifest['source_correspondence_verified'] = report['source_correspondence_verified']
manifest['corresponding_source_complete'] = report['corresponding_source_complete']
manifest['source_review'] = {'asset': report_path.name, 'sha256': hashlib.sha256(report_path.read_bytes()).hexdigest(),
                             'bytes': report_path.stat().st_size}
manifest_path.write_text(json.dumps(manifest, sort_keys=True, indent=2) + '\n')
PYCODE
rm -f "$RUNNER_TEMP/wsx-release-assets/SHA256SUMS"
(cd "$RUNNER_TEMP/wsx-release-assets" && find . -maxdepth 1 -type f -print0 | sort -z | xargs -0 sha256sum > "SHA256SUMS-$TARGET_VERSION-$run_id")
: "${WSX_KERNEL_ARTIFACT_ROOT:?}"
mkdir -p "$WSX_KERNEL_ARTIFACT_ROOT"
mv "$RUNNER_TEMP/wsx-release-assets" "$WSX_KERNEL_ARTIFACT_ROOT/$TARGET_VERSION"
echo "Rebuilt assets prepared for release: $TARGET_VERSION" >> "$GITHUB_STEP_SUMMARY"
