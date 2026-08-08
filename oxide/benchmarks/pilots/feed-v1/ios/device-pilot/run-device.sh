#!/bin/bash

set -uo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]
then
   echo "usage: $0 <CoreDevice-ID> <new-result-root> [smoke|full]" >&2
   exit 2
fi

CORE_DEVICE_ID="$1"
RESULT_ROOT="$2"
MODE="${3:-smoke}"
XCODE_DEVICE_ID=""
EXPECTED_CORE_DEVICE_ID="1DEDF2A3-EC8E-5FCC-A437-8BD3A6F3D659"
EXPECTED_XCODE_DEVICE_ID="00008150-001529C434F8401C"
if [[ "$CORE_DEVICE_ID" != "$EXPECTED_CORE_DEVICE_ID" ]]
then
   echo "feed-v1 publication is frozen to CoreDevice $EXPECTED_CORE_DEVICE_ID" >&2
   exit 2
fi
DEVELOPMENT_TEAM_ID="${OXIDE_IOS_DEVELOPMENT_TEAM:-${DEVELOPMENT_TEAM:-}}"
if [[ "$MODE" != "smoke" && "$MODE" != "full" ]]
then
   echo "mode must be smoke or full" >&2
   exit 2
fi
if [[ ! "$DEVELOPMENT_TEAM_ID" =~ ^[[:upper:][:digit:]]{10}$ ]]
then
   echo "set OXIDE_IOS_DEVELOPMENT_TEAM to the 10-character iOS signing team" >&2
   exit 2
fi
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FEED_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
if ! REPOSITORY_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
then
   echo "feed-v1 source is not in a Git worktree" >&2
   exit 2
fi
REPOSITORY_ROOT="$(cd "$REPOSITORY_ROOT" && pwd -P)"
if ! RESULT_PARENT="$(cd "$(dirname "$RESULT_ROOT")" && pwd -P)"
then
   echo "result-root parent must already exist" >&2
   exit 2
fi
RESULT_NAME="$(basename "$RESULT_ROOT")"
if [[ -z "$RESULT_NAME" || "$RESULT_NAME" == "." || "$RESULT_NAME" == ".." ]]
then
   echo "result root must name a new directory" >&2
   exit 2
fi
RESULT_ROOT="$RESULT_PARENT/$RESULT_NAME"
if [[ -e "$RESULT_ROOT" ]]
then
   echo "result root must not already exist: $RESULT_ROOT" >&2
   exit 2
fi
case "$RESULT_ROOT" in
   "$REPOSITORY_ROOT"|"$REPOSITORY_ROOT"/*)
      echo "result root must be outside the Git worktree: $RESULT_ROOT" >&2
      exit 2
      ;;
esac
if [[ -n "$(git -C "$REPOSITORY_ROOT" status --porcelain=v1 --untracked-files=all)" ]]
then
   echo "feed-v1 device evidence requires a clean Git worktree" >&2
   exit 2
fi
if ! REPOSITORY_REF="$(git -C "$REPOSITORY_ROOT" symbolic-ref --quiet HEAD)" \
   || [[ "$REPOSITORY_REF" != refs/heads/* ]] \
   || ! REPOSITORY_HEAD="$(git -C "$REPOSITORY_ROOT" rev-parse --verify 'HEAD^{commit}')" \
   || ! REPOSITORY_TREE="$(git -C "$REPOSITORY_ROOT" rev-parse --verify 'HEAD^{tree}')"
then
   echo "feed-v1 device evidence requires a named Git branch and committed HEAD" >&2
   exit 2
fi
if [[ ! "$REPOSITORY_HEAD" =~ ^[[:xdigit:]]{40}$ || ! "$REPOSITORY_TREE" =~ ^[[:xdigit:]]{40}$ ]]
then
   echo "Git commit or tree identity is malformed" >&2
   exit 2
fi

verify_source_snapshot()
{
   local stage="$1"
   local current_ref
   local current_head
   local current_tree
   if [[ -n "$(git -C "$REPOSITORY_ROOT" status --porcelain=v1 --untracked-files=all)" ]] \
      || ! current_ref="$(git -C "$REPOSITORY_ROOT" symbolic-ref --quiet HEAD)" \
      || ! current_head="$(git -C "$REPOSITORY_ROOT" rev-parse --verify 'HEAD^{commit}')" \
      || ! current_tree="$(git -C "$REPOSITORY_ROOT" rev-parse --verify 'HEAD^{tree}')" \
      || [[ "$current_ref" != "$REPOSITORY_REF" ]] \
      || [[ "$current_head" != "$REPOSITORY_HEAD" ]] \
      || [[ "$current_tree" != "$REPOSITORY_TREE" ]]
   then
      echo "Git source snapshot changed or became dirty during $stage" >&2
      return 1
   fi
}

BUILD_ROOT="$(mktemp -d /tmp/oxide-feed-v1-build.XXXXXX)"
REDUCER_RUN_ROOT="$(mktemp -d /tmp/oxide-feed-v1-reducer.XXXXXX)"
GENERATED_PROJECT_ROOT="$BUILD_ROOT/project"
PROJECT="$GENERATED_PROJECT_ROOT/FeedV1Pilot.xcodeproj"
DERIVED_DATA="$BUILD_ROOT/DerivedData"
RUST_TARGET="$BUILD_ROOT/rust-ios"
REDUCER_TARGET="$BUILD_ROOT/reducer-target"
RAW_ROOT="$RESULT_ROOT/raw"
ATTACHMENT_ROOT="$RAW_ROOT/attachments"
PROVENANCE_ROOT="$RAW_ROOT/provenance"
BUILD_PROVENANCE="$BUILD_ROOT/build-provenance.json"
CONTROLLER_BUNDLE_ID="com.oxide.feed-v1.controller.xctrunner"
MAX_BUILD_ROOT_BYTES=4294967296
MAX_RETAINED_EVIDENCE_BYTES=536870912
MAX_RETAINED_FILE_COUNT=512
MAX_RETAINED_FILE_BYTES=134217728
MAX_RESULT_BUNDLE_BYTES=536870912
SCHEME="FeedV1PilotSmoke"
EXPECTED_ATTACHMENTS=12
if [[ "$MODE" == "full" ]]
then
   SCHEME="FeedV1Pilot"
fi

if ! mkdir -p "$RAW_ROOT" "$ATTACHMENT_ROOT" "$PROVENANCE_ROOT"
then
   echo "could not create the external result root" >&2
   exit 1
fi
APPS_UNINSTALLED=false
CONTROLLER_UNINSTALLED=false
UIKIT_PROCESS_ABSENT=false
OXIDE_PROCESS_ABSENT=false
CONTROLLER_PROCESS_ABSENT=false
PRELAUNCH_FUSES_ADMITTED=false
SOURCE_SNAPSHOT_PRESERVED=false
BUILD_REMOVED=false
RESULT_BUNDLE_REMOVED=false
REDUCER_REMOVED=false
TEST_SUCCEEDED=false
SMOKE_TEST_SUCCEEDED=false
SMOKE_ADMITTED=false
PRIMARY_STARTED=false
PRIMARY_TEST_SUCCEEDED=false
FIRST_FAILURE_STAGE=""
FIRST_FAILURE_REASON=""
VERIFIED_ATTACHMENT_COUNT=0
TEST_STATUS=1
BUILD_ROOT_BYTES=0
RESULT_BUNDLE_BYTES=0
RETAINED_FILE_COUNT=0
LARGEST_RETAINED_FILE_BYTES=0
START_SECONDS="$(date +%s)"

record_failure()
{
   if [[ -z "$FIRST_FAILURE_STAGE" ]]
   then
      FIRST_FAILURE_STAGE="$1"
      FIRST_FAILURE_REASON="$2"
   fi
}

cleanup_backstop()
{
   if [[ -d "$BUILD_ROOT" ]]
   then
      remove_device_processes backstop uikit FeedV1UIKit >/dev/null 2>&1 || true
      remove_device_processes backstop oxide FeedV1Oxide >/dev/null 2>&1 || true
      remove_controller_processes backstop >/dev/null 2>&1 || true
   fi
   xcrun devicectl device uninstall app --device "$CORE_DEVICE_ID" com.oxide.feed-v1.uikit --quiet >/dev/null 2>&1 || true
   xcrun devicectl device uninstall app --device "$CORE_DEVICE_ID" com.oxide.feed-v1.oxide --quiet >/dev/null 2>&1 || true
   xcrun devicectl device uninstall app --device "$CORE_DEVICE_ID" "$CONTROLLER_BUNDLE_ID" --quiet >/dev/null 2>&1 || true
   if [[ "$BUILD_ROOT" == /tmp/oxide-feed-v1-build.* && -d "$BUILD_ROOT" ]]
   then
      /bin/rm -rf -- "$BUILD_ROOT"
   fi
   if [[ "$REDUCER_RUN_ROOT" == /tmp/oxide-feed-v1-reducer.* && -d "$REDUCER_RUN_ROOT" ]]
   then
      /bin/rm -rf -- "$REDUCER_RUN_ROOT"
   fi
}

require_unlocked_device()
{
   local output="$1"
   local passcode_required
   if ! xcrun devicectl device info lockState --device "$CORE_DEVICE_ID" --json-output "$output" --quiet
   then
      echo "physical device lock-state query failed" >&2
      return 1
   fi
   if ! passcode_required="$(/usr/bin/plutil -extract result.passcodeRequired raw -o - "$output")"
   then
      echo "physical device lock-state response is malformed" >&2
      return 1
   fi
   if [[ "$passcode_required" != "false" ]]
   then
      echo "physical device is locked; unlock it before starting feed-v1" >&2
      return 1
   fi
}

resolve_xcode_device_id()
{
   local details="$1"
   local udid
   local reality
   local platform
   local boot_state
   if ! udid="$(/usr/bin/plutil -extract result.hardwareProperties.udid raw -o - "$details")" \
      || ! reality="$(/usr/bin/plutil -extract result.hardwareProperties.reality raw -o - "$details")" \
      || ! platform="$(/usr/bin/plutil -extract result.hardwareProperties.platform raw -o - "$details")" \
      || ! boot_state="$(/usr/bin/plutil -extract result.deviceProperties.bootState raw -o - "$details")"
   then
      echo "physical device details do not contain the Xcode identity contract" >&2
      return 1
   fi
   if [[ "$reality" != "physical" || "$platform" != "iOS" || "$boot_state" != "booted" ]]
   then
      echo "selected CoreDevice is not a booted physical iPhone" >&2
      return 1
   fi
   if [[ ! "$udid" =~ ^[[:xdigit:]]{8}-[[:xdigit:]]{16}$ ]]
   then
      echo "physical device hardware UDID is malformed" >&2
      return 1
   fi
   if [[ "$udid" != "$EXPECTED_XCODE_DEVICE_ID" ]]
   then
      echo "CoreDevice $CORE_DEVICE_ID resolved to unexpected hardware $udid" >&2
      return 1
   fi
   printf '%s\n' "$udid"
}

verify_app_contract()
{
   local app="$1"
   local label="$2"
   local plist="$app/Info.plist"
   local executable
   local architectures
   local supported_platforms
   local device_capabilities
   local minimum_frame_duration_disabled
   local full_screen
   local orientations
   local status_hidden
   local controller_status
   if [[ ! -f "$plist" ]]
   then
      echo "$label product has no Info.plist" >&2
      return 1
   fi
   if ! executable="$(/usr/bin/plutil -extract CFBundleExecutable raw -o - "$plist")" \
      || [[ "$executable" == */* || ! -f "$app/$executable" ]]
   then
      echo "$label product has no admitted executable" >&2
      return 1
   fi
   if ! architectures="$(lipo -archs "$app/$executable")" || [[ "$architectures" != "arm64" ]]
   then
      echo "$label product is not exactly arm64" >&2
      return 1
   fi
   if ! supported_platforms="$(/usr/bin/plutil -extract CFBundleSupportedPlatforms json -o - "$plist")" \
      || [[ "$supported_platforms" != '["iPhoneOS"]' ]]
   then
      echo "$label product is not iPhoneOS-only" >&2
      return 1
   fi
   if ! device_capabilities="$(/usr/bin/plutil -extract UIRequiredDeviceCapabilities json -o - "$plist")" \
      || [[ "$device_capabilities" != '["arm64"]' ]]
   then
      echo "$label product does not require arm64" >&2
      return 1
   fi
   if ! minimum_frame_duration_disabled="$(/usr/bin/plutil -extract CADisableMinimumFrameDurationOnPhone raw -o - "$plist")" \
      || [[ "$minimum_frame_duration_disabled" != "true" ]]
   then
      echo "$label product does not enable native ProMotion frame durations" >&2
      return 1
   fi
   if ! full_screen="$(/usr/bin/plutil -extract UIRequiresFullScreen raw -o - "$plist")" \
      || [[ "$full_screen" != "true" ]]
   then
      echo "$label product does not require full-screen presentation" >&2
      return 1
   fi
   if ! orientations="$(/usr/bin/plutil -extract UISupportedInterfaceOrientations json -o - "$plist")" \
      || [[ "$orientations" != '["UIInterfaceOrientationPortrait"]' ]]
   then
      echo "$label product does not freeze portrait-only presentation" >&2
      return 1
   fi
   if ! status_hidden="$(/usr/bin/plutil -extract UIStatusBarHidden raw -o - "$plist")" \
      || [[ "$status_hidden" != "true" ]]
   then
      echo "$label product does not hide the status bar" >&2
      return 1
   fi
   if ! controller_status="$(/usr/bin/plutil -extract UIViewControllerBasedStatusBarAppearance raw -o - "$plist")" \
      || [[ "$controller_status" != "false" ]]
   then
      echo "$label product does not freeze application-owned status-bar appearance" >&2
      return 1
   fi
}

verify_uikit_resource_contract()
{
   local app="$1"
   local name
   for name in Asap-Regular.ttf Asap-Bold.ttf
   do
      if [[ ! -f "$app/$name" ]]
      then
         echo "UIKit product is missing frozen font $name" >&2
         return 1
      fi
      if ! cmp -s "$REPOSITORY_ROOT/oxide/crates/ui-core/assets/$name" "$app/$name"
      then
         echo "UIKit product font $name differs from the frozen source bytes" >&2
         return 1
      fi
   done
}

verify_controller_contract()
{
   local runner="$1"
   local xctest="$2"
   local runner_plist="$runner/Info.plist"
   local xctest_plist="$xctest/Info.plist"
   local runner_bundle_id
   local runner_executable
   local xctest_bundle_id
   local xctest_executable
   local architectures
   if [[ ! -f "$runner_plist" || ! -f "$xctest_plist" ]]
   then
      echo "controller products do not contain processed Info.plists" >&2
      return 1
   fi
   if ! runner_bundle_id="$(/usr/bin/plutil -extract CFBundleIdentifier raw -o - "$runner_plist")" \
      || [[ "$runner_bundle_id" != "$CONTROLLER_BUNDLE_ID" ]] \
      || ! runner_executable="$(/usr/bin/plutil -extract CFBundleExecutable raw -o - "$runner_plist")" \
      || [[ "$runner_executable" != "FeedV1Controller-Runner" || ! -f "$runner/$runner_executable" ]]
   then
      echo "controller runner identity differs from the frozen contract" >&2
      return 1
   fi
   if ! xctest_bundle_id="$(/usr/bin/plutil -extract CFBundleIdentifier raw -o - "$xctest_plist")" \
      || [[ "$xctest_bundle_id" != "com.oxide.feed-v1.controller" ]] \
      || ! xctest_executable="$(/usr/bin/plutil -extract CFBundleExecutable raw -o - "$xctest_plist")" \
      || [[ "$xctest_executable" != "FeedV1Controller" || ! -f "$xctest/$xctest_executable" ]]
   then
      echo "controller xctest identity differs from the frozen contract" >&2
      return 1
   fi
   if ! architectures="$(lipo -archs "$runner/$runner_executable")" || [[ "$architectures" != "arm64" ]]
   then
      echo "controller runner is not exactly arm64" >&2
      return 1
   fi
   if ! architectures="$(lipo -archs "$xctest/$xctest_executable")" || [[ "$architectures" != "arm64" ]]
   then
      echo "controller xctest is not exactly arm64" >&2
      return 1
   fi
}

remove_device_processes()
{
   local stage="$1"
   local label="$2"
   local executable="$3"
   local before="$RAW_ROOT/$stage-$label-process-before.json"
   local after="$RAW_ROOT/$stage-$label-process-after.json"
   local index=0
   local pid
   local processes
   if ! xcrun devicectl device info processes --device "$CORE_DEVICE_ID" \
      --filter "executable.absoluteString ENDSWITH '/$executable'" \
      --json-output "$before" --quiet
   then
      echo "could not inspect the $label process during $stage" >&2
      return 1
   fi
   while pid="$(/usr/bin/plutil -extract "result.runningProcesses.$index.processIdentifier" raw -o - "$before" 2>/dev/null)"
   do
      if [[ ! "$pid" =~ ^[[:digit:]]+$ ]] \
         || ! xcrun devicectl device process terminate --device "$CORE_DEVICE_ID" --pid "$pid" --quiet
      then
         echo "could not terminate $label process $pid during $stage" >&2
         return 1
      fi
      index=$((index + 1))
   done
   if ! xcrun devicectl device info processes --device "$CORE_DEVICE_ID" \
      --filter "executable.absoluteString ENDSWITH '/$executable'" \
      --json-output "$after" --quiet \
      || ! processes="$(/usr/bin/plutil -extract result.runningProcesses json -o - "$after")" \
      || [[ "$processes" != "[]" ]]
   then
      echo "$label process survived cleanup during $stage" >&2
      return 1
   fi
}

remove_controller_processes()
{
   remove_device_processes "$1" controller FeedV1Controller-Runner
}

remove_existing_app()
{
   local bundle_id="$1"
   local label="$2"
   local stage="$3"
   local before="$RAW_ROOT/$stage-$label-before.json"
   local after="$RAW_ROOT/$stage-$label-after.json"
   local apps
   if ! xcrun devicectl device info apps --device "$CORE_DEVICE_ID" --bundle-id "$bundle_id" \
      --json-output "$before" --quiet
   then
      echo "could not inspect $bundle_id installation during $stage" >&2
      return 1
   fi
   if ! apps="$(/usr/bin/plutil -extract result.apps json -o - "$before")"
   then
      echo "app query for $bundle_id is malformed during $stage" >&2
      return 1
   fi
   if [[ "$apps" == "[]" ]]
   then
      return 0
   fi
   if ! xcrun devicectl device uninstall app --device "$CORE_DEVICE_ID" "$bundle_id" --quiet
   then
      echo "could not remove $bundle_id installation during $stage" >&2
      return 1
   fi
   if ! xcrun devicectl device info apps --device "$CORE_DEVICE_ID" --bundle-id "$bundle_id" \
      --json-output "$after" --quiet
   then
      echo "could not verify removal of $bundle_id" >&2
      return 1
   fi
   if ! apps="$(/usr/bin/plutil -extract result.apps json -o - "$after")" || [[ "$apps" != "[]" ]]
   then
      echo "$bundle_id installation survived removal during $stage" >&2
      return 1
   fi
}

measure_tree()
{
   local root="$1"
   local kib
   if ! kib="$(du -sk "$root" | awk '{print $1}')" \
      || ! TREE_FILE_COUNT="$(find "$root" -type f | wc -l | tr -d ' ')" \
      || ! TREE_LARGEST_FILE_BYTES="$(find "$root" -type f -exec stat -f '%z' {} + | awk 'BEGIN { maximum = 0 } { if ($1 > maximum) maximum = $1 } END { print maximum }')" \
      || [[ ! "$kib" =~ ^[[:digit:]]+$ || ! "$TREE_FILE_COUNT" =~ ^[[:digit:]]+$ || ! "$TREE_LARGEST_FILE_BYTES" =~ ^[[:digit:]]+$ ]]
   then
      return 1
   fi
   TREE_BYTES=$((kib * 1024))
}

check_directory_bytes()
{
   local root="$1"
   local label="$2"
   local maximum="$3"
   if ! measure_tree "$root" || [[ "$TREE_BYTES" -gt "$maximum" ]]
   then
      echo "$label exceeded its predeclared $maximum-byte fuse" >&2
      return 1
   fi
}

check_retained_evidence_fuses()
{
   if ! measure_tree "$RESULT_ROOT" \
      || [[ "$TREE_BYTES" -gt "$MAX_RETAINED_EVIDENCE_BYTES" ]] \
      || [[ "$TREE_FILE_COUNT" -gt "$MAX_RETAINED_FILE_COUNT" ]] \
      || [[ "$TREE_LARGEST_FILE_BYTES" -gt "$MAX_RETAINED_FILE_BYTES" ]]
   then
      echo "retained evidence exceeded its predeclared byte, file-count, or per-file fuse" >&2
      return 1
   fi
   RETAINED_FILE_COUNT="$TREE_FILE_COUNT"
   LARGEST_RETAINED_FILE_BYTES="$TREE_LARGEST_FILE_BYTES"
}

sha256_file()
{
   /usr/bin/shasum -a 256 "$1" | awk '{print $1}'
}

capture_toolchain_provenance()
{
   if [[ "$(/usr/bin/arch)" != "arm64" ]]
   then
      echo "feed-v1 formal execution requires a native arm64 host process" >&2
      return 1
   fi
   if ! xcrun xcodebuild -version >"$PROVENANCE_ROOT/xcode-version.txt" 2>&1 \
      || ! xcrun --sdk iphoneos --show-sdk-version >"$PROVENANCE_ROOT/iphoneos-sdk-version.txt" 2>&1 \
      || ! xcrun --sdk iphoneos --show-sdk-build-version >"$PROVENANCE_ROOT/iphoneos-sdk-build.txt" 2>&1 \
      || ! rustc -vV >"$PROVENANCE_ROOT/rustc-version.txt" 2>&1 \
      || ! cargo --version >"$PROVENANCE_ROOT/cargo-version.txt" 2>&1 \
      || ! cargo metadata --locked --filter-platform aarch64-apple-ios --format-version 1 \
         --manifest-path "$FEED_ROOT/ios/oxide-feed-app/Cargo.toml" \
         >"$PROVENANCE_ROOT/production-cargo-metadata.json" 2>"$PROVENANCE_ROOT/production-cargo-metadata.log"
   then
      echo "toolchain or production dependency provenance capture failed" >&2
      return 1
   fi
   verify_source_snapshot "toolchain provenance capture"
}

capture_signing_identity()
{
   local product="$1"
   local label="$2"
   local output="$PROVENANCE_ROOT/$label-codesign.txt"
   local authority
   local team_identifier
   local cdhash
   if ! /usr/bin/codesign -d --verbose=4 "$product" >/dev/null 2>"$output" \
      || ! authority="$(sed -n 's/^Authority=//p' "$output" | head -1)" \
      || ! team_identifier="$(sed -n 's/^TeamIdentifier=//p' "$output" | head -1)" \
      || ! cdhash="$(sed -n 's/^CDHash=//p' "$output" | head -1)"
   then
      echo "$label signing identity capture failed" >&2
      return 1
   fi
   if [[ "$authority" != "Apple Development:"* ]] \
      || [[ "$team_identifier" != "$DEVELOPMENT_TEAM_ID" ]] \
      || [[ ! "$cdhash" =~ ^[[:xdigit:]]{40}([[:xdigit:]]{24})?$ ]]
   then
      echo "$label signing identity does not match the frozen development-signing contract" >&2
      return 1
   fi
}

insert_signing_identity()
{
   local plist="$1"
   local key="$2"
   local output="$3"
   local authority
   local team_identifier
   local cdhash
   authority="$(sed -n 's/^Authority=//p' "$output" | head -1)"
   team_identifier="$(sed -n 's/^TeamIdentifier=//p' "$output" | head -1)"
   cdhash="$(sed -n 's/^CDHash=//p' "$output" | head -1)"
   /usr/bin/plutil -insert "$key" -dictionary "$plist" \
      && /usr/bin/plutil -insert "$key.authority" -string "$authority" "$plist" \
      && /usr/bin/plutil -insert "$key.team_identifier" -string "$team_identifier" "$plist" \
      && /usr/bin/plutil -insert "$key.cdhash" -string "$cdhash" "$plist"
}

write_build_provenance()
{
   local details="$RAW_ROOT/device-before.json"
   local plist="$BUILD_ROOT/build-provenance.plist"
   local device_model
   local device_product_type
   local os_version
   local os_build
   local xcode_version
   local xcode_build
   local sdk_version
   local sdk_build
   local rustc_release
   local rustc_commit_hash
   local rustc_host
   local cargo_version
   local build_settings_sha256
   local cargo_lock_sha256
   local cargo_metadata_sha256
   if ! device_model="$(/usr/bin/plutil -extract result.hardwareProperties.marketingName raw -o - "$details")" \
      || ! device_product_type="$(/usr/bin/plutil -extract result.hardwareProperties.productType raw -o - "$details")" \
      || ! os_version="$(/usr/bin/plutil -extract result.deviceProperties.osVersionNumber raw -o - "$details")" \
      || ! os_build="$(/usr/bin/plutil -extract result.deviceProperties.osBuildUpdate raw -o - "$details")" \
      || ! xcode_version="$(sed -n 's/^Xcode //p' "$PROVENANCE_ROOT/xcode-version.txt")" \
      || ! xcode_build="$(sed -n 's/^Build version //p' "$PROVENANCE_ROOT/xcode-version.txt")" \
      || ! sdk_version="$(head -1 "$PROVENANCE_ROOT/iphoneos-sdk-version.txt")" \
      || ! sdk_build="$(head -1 "$PROVENANCE_ROOT/iphoneos-sdk-build.txt")" \
      || ! rustc_release="$(sed -n 's/^release: //p' "$PROVENANCE_ROOT/rustc-version.txt")" \
      || ! rustc_commit_hash="$(sed -n 's/^commit-hash: //p' "$PROVENANCE_ROOT/rustc-version.txt")" \
      || ! rustc_host="$(sed -n 's/^host: //p' "$PROVENANCE_ROOT/rustc-version.txt")" \
      || ! cargo_version="$(head -1 "$PROVENANCE_ROOT/cargo-version.txt")" \
      || ! build_settings_sha256="$(sha256_file "$PROVENANCE_ROOT/release-build-settings.txt")" \
      || ! cargo_lock_sha256="$(sha256_file "$REPOSITORY_ROOT/oxide/Cargo.lock")" \
      || ! cargo_metadata_sha256="$(sha256_file "$PROVENANCE_ROOT/production-cargo-metadata.json")"
   then
      echo "build provenance inputs are incomplete" >&2
      return 1
   fi
   if [[ -z "$device_model" || -z "$device_product_type" || -z "$os_version" || -z "$os_build" \
      || -z "$xcode_version" || -z "$xcode_build" || -z "$sdk_version" || -z "$sdk_build" \
      || -z "$rustc_release" || ! "$rustc_commit_hash" =~ ^[[:xdigit:]]{40}$ \
      || "$rustc_host" != "aarch64-apple-darwin" || "$cargo_version" != "cargo "* \
      || ! "$build_settings_sha256" =~ ^[[:xdigit:]]{64}$ \
      || ! "$cargo_lock_sha256" =~ ^[[:xdigit:]]{64}$ \
      || ! "$cargo_metadata_sha256" =~ ^[[:xdigit:]]{64}$ ]]
   then
      echo "build provenance values are malformed" >&2
      return 1
   fi
   if ! /usr/bin/plutil -create xml1 "$plist" \
      || ! /usr/bin/plutil -insert schema -string oxide.feed-v1.build-provenance "$plist" \
      || ! /usr/bin/plutil -insert schema_revision -integer 1 "$plist" \
      || ! /usr/bin/plutil -insert core_device_id -string "$CORE_DEVICE_ID" "$plist" \
      || ! /usr/bin/plutil -insert hardware_udid -string "$XCODE_DEVICE_ID" "$plist" \
      || ! /usr/bin/plutil -insert device_model -string "$device_model" "$plist" \
      || ! /usr/bin/plutil -insert device_product_type -string "$device_product_type" "$plist" \
      || ! /usr/bin/plutil -insert os_version -string "$os_version" "$plist" \
      || ! /usr/bin/plutil -insert os_build -string "$os_build" "$plist" \
      || ! /usr/bin/plutil -insert maximum_refresh_hz -integer 120 "$plist" \
      || ! /usr/bin/plutil -insert xcode_version -string "$xcode_version" "$plist" \
      || ! /usr/bin/plutil -insert xcode_build -string "$xcode_build" "$plist" \
      || ! /usr/bin/plutil -insert iphoneos_sdk_version -string "$sdk_version" "$plist" \
      || ! /usr/bin/plutil -insert iphoneos_sdk_build -string "$sdk_build" "$plist" \
      || ! /usr/bin/plutil -insert rustc_release -string "$rustc_release" "$plist" \
      || ! /usr/bin/plutil -insert rustc_commit_hash -string "$rustc_commit_hash" "$plist" \
      || ! /usr/bin/plutil -insert rustc_host -string "$rustc_host" "$plist" \
      || ! /usr/bin/plutil -insert cargo_version -string "$cargo_version" "$plist" \
      || ! /usr/bin/plutil -insert release_build_settings_sha256 -string "$build_settings_sha256" "$plist" \
      || ! /usr/bin/plutil -insert production_cargo_lock_sha256 -string "$cargo_lock_sha256" "$plist" \
      || ! /usr/bin/plutil -insert production_cargo_metadata_sha256 -string "$cargo_metadata_sha256" "$plist" \
      || ! insert_signing_identity "$plist" uikit_signing "$PROVENANCE_ROOT/uikit-codesign.txt" \
      || ! insert_signing_identity "$plist" oxide_signing "$PROVENANCE_ROOT/oxide-codesign.txt" \
      || ! insert_signing_identity "$plist" controller_runner_signing "$PROVENANCE_ROOT/controller-runner-codesign.txt" \
      || ! insert_signing_identity "$plist" controller_xctest_signing "$PROVENANCE_ROOT/controller-xctest-codesign.txt" \
      || ! /usr/bin/plutil -convert json -o "$BUILD_PROVENANCE" "$plist"
   then
      echo "build provenance serialization failed" >&2
      return 1
   fi
}

verify_fixture_contracts()
{
   local swift_check="$BUILD_ROOT/feed-v1-swift-contract-check"
   local swift_log="$RAW_ROOT/swift-contract-check.log"
   local rust_log="$RAW_ROOT/rust-contract-check.log"
   if ! xcrun swiftc -parse-as-library \
      -DFEED_V1_CONTRACT_CHECK_MAIN \
      "$FEED_ROOT/ios/FeedV1Contract.swift" \
      "$FEED_ROOT/ios/FeedV1OptimizedUIKitConfiguration.swift" \
      "$FEED_ROOT/ios/FeedV1ContractCheckMain.swift" \
      -o "$swift_check" \
      >"$RAW_ROOT/swift-contract-build.log" 2>&1 \
      || ! "$swift_check" >"$swift_log" 2>&1
   then
      echo "Swift canonical fixture preflight failed" >&2
      return 1
   fi
   for expected in \
      "canonical_sha256=a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473" \
      "canonical_byte_count=717745" \
      "content_extent_points=237460" \
      "maximum_content_offset_points=236616"
   do
      if ! grep -Fqx "$expected" "$swift_log"
      then
         echo "Swift canonical fixture preflight omitted $expected" >&2
         return 1
      fi
   done
   if ! CARGO_TARGET_DIR="$BUILD_ROOT/contract-rust-target" cargo test --locked \
      --manifest-path "$FEED_ROOT/ios/oxide-feed-app/Cargo.toml" \
      --test contract_tests -- \
      rust_recipe_reproduces_the_complete_swift_canonical_identity --exact \
      >"$rust_log" 2>&1
   then
      echo "Rust canonical fixture preflight failed" >&2
      return 1
   fi
   verify_source_snapshot "canonical fixture preflight"
}

trap cleanup_backstop EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

export FEED_V1_SOURCE_ROOT="$SCRIPT_DIR"
if ! verify_fixture_contracts \
   || ! capture_toolchain_provenance \
   || ! mkdir -p "$GENERATED_PROJECT_ROOT" \
   || ! xcodegen generate \
      --spec "$SCRIPT_DIR/project.yml" \
      --project "$GENERATED_PROJECT_ROOT" \
      --project-root "$SCRIPT_DIR" \
   || [[ ! -d "$PROJECT" ]] \
   || ! verify_source_snapshot "external Xcode project generation"
then
   echo "xcodegen did not produce the external project from the committed source snapshot" >&2
   exit 1
fi

xcrun devicectl device info details --device "$CORE_DEVICE_ID" --json-output "$RAW_ROOT/device-before.json" --quiet
if [[ $? -ne 0 ]]
then
   echo "physical device preflight failed" >&2
   exit 1
fi
if ! XCODE_DEVICE_ID="$(resolve_xcode_device_id "$RAW_ROOT/device-before.json")"
then
   exit 1
fi
if ! require_unlocked_device "$RAW_ROOT/lock-before-build.json"
then
   exit 1
fi
if ! xcrun xcodebuild -project "$PROJECT" -scheme "$SCHEME" -showdestinations \
   >"$RAW_ROOT/xcode-destinations.txt" 2>&1
then
   echo "Xcode destination discovery failed" >&2
   exit 1
fi
XCODE_DESTINATIONS="$(<"$RAW_ROOT/xcode-destinations.txt")"
if [[ "$XCODE_DESTINATIONS" != *"id:$XCODE_DEVICE_ID"* ]]
then
   echo "resolved physical hardware UDID is not an Xcode destination" >&2
   exit 1
fi
if ! xcrun xcodebuild \
   -project "$PROJECT" \
   -scheme "$SCHEME" \
   -configuration Release \
   -destination "id=$XCODE_DEVICE_ID" \
   -derivedDataPath "$DERIVED_DATA" \
   DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM_ID" \
   CODE_SIGN_STYLE=Automatic \
   CODE_SIGN_IDENTITY="Apple Development" \
   -showBuildSettings >"$PROVENANCE_ROOT/release-build-settings.txt" 2>&1 \
   || ! grep -Fq "    ARCHS = arm64" "$PROVENANCE_ROOT/release-build-settings.txt" \
   || grep -Eq '^ +ARCHS = .*x86_64' "$PROVENANCE_ROOT/release-build-settings.txt" \
   || ! grep -Fq "    CONFIGURATION = Release" "$PROVENANCE_ROOT/release-build-settings.txt" \
   || ! grep -Fq "    PLATFORM_NAME = iphoneos" "$PROVENANCE_ROOT/release-build-settings.txt" \
   || ! grep -Fq "    DEVELOPMENT_TEAM = $DEVELOPMENT_TEAM_ID" "$PROVENANCE_ROOT/release-build-settings.txt"
then
   echo "resolved Release arm64 build settings are not publication-admissible" >&2
   exit 1
fi

export FEED_V1_CARGO_TARGET_DIR="$RUST_TARGET"
build_for_testing()
{
   local scheme="$1"
   local log="$2"
   xcrun xcodebuild \
      -project "$PROJECT" \
      -scheme "$scheme" \
      -configuration Release \
      -destination "id=$XCODE_DEVICE_ID" \
      -derivedDataPath "$DERIVED_DATA" \
      -allowProvisioningUpdates \
      -allowProvisioningDeviceRegistration \
      DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM_ID" \
      CODE_SIGN_STYLE=Automatic \
      CODE_SIGN_IDENTITY="Apple Development" \
      build-for-testing 2>&1 | tee "$log"
   return "${PIPESTATUS[0]}"
}

if [[ "$MODE" == "full" ]]
then
   build_for_testing FeedV1PilotSmoke "$RAW_ROOT/build-smoke.log" \
      && build_for_testing FeedV1Pilot "$RAW_ROOT/build-primary.log"
else
   build_for_testing "$SCHEME" "$RAW_ROOT/build.log"
fi
if [[ $? -ne 0 ]]
then
   echo "arm64 device build failed" >&2
   exit 1
fi

UIKIT_APP="$DERIVED_DATA/Build/Products/Release-iphoneos/FeedV1UIKit.app"
OXIDE_APP="$DERIVED_DATA/Build/Products/Release-iphoneos/FeedV1Oxide.app"
CONTROLLER_RUNNER="$DERIVED_DATA/Build/Products/Release-iphoneos/FeedV1Controller-Runner.app"
CONTROLLER_XCTEST="$CONTROLLER_RUNNER/PlugIns/FeedV1Controller.xctest"
if [[ ! -d "$UIKIT_APP" || ! -d "$OXIDE_APP" || ! -d "$CONTROLLER_RUNNER" || ! -d "$CONTROLLER_XCTEST" ]]
then
   echo "expected app and controller products were not built" >&2
   exit 1
fi
if ! verify_app_contract "$UIKIT_APP" UIKit \
   || ! verify_uikit_resource_contract "$UIKIT_APP" \
   || ! verify_app_contract "$OXIDE_APP" Oxide \
   || ! verify_controller_contract "$CONTROLLER_RUNNER" "$CONTROLLER_XCTEST" \
   || ! capture_signing_identity "$UIKIT_APP" uikit \
   || ! capture_signing_identity "$OXIDE_APP" oxide \
   || ! capture_signing_identity "$CONTROLLER_RUNNER" controller-runner \
   || ! capture_signing_identity "$CONTROLLER_XCTEST" controller-xctest \
   || ! write_build_provenance \
   || ! verify_source_snapshot "arm64 device build"
then
   exit 1
fi

CARGO_TARGET_DIR="$REDUCER_TARGET" cargo build --locked --profile feed-v1-reducer \
   --manifest-path "$FEED_ROOT/reducer/Cargo.toml"
if [[ $? -ne 0 ]]
then
   echo "reducer build failed" >&2
   exit 1
fi
cp "$REDUCER_TARGET/feed-v1-reducer/oxide-feed-v1-reducer" "$REDUCER_RUN_ROOT/oxide-feed-v1-reducer"
REDUCER="$REDUCER_RUN_ROOT/oxide-feed-v1-reducer"

if ! verify_source_snapshot "evidence-manifest capture"
then
   exit 1
fi
"$REDUCER" manifest \
   "$FEED_ROOT" \
   "$REPOSITORY_ROOT" \
   "$UIKIT_APP" \
   "$OXIDE_APP" \
   "$CONTROLLER_RUNNER" \
   "$CONTROLLER_XCTEST" \
   "$BUILD_PROVENANCE" \
   "$RAW_ROOT/evidence-manifest.json"
if [[ $? -ne 0 ]]
then
   echo "evidence manifest failed" >&2
   exit 1
fi

verify_evidence_manifest_snapshot()
{
   local stage="$1"
   local candidate="$BUILD_ROOT/evidence-manifest-$stage.json"
   "$REDUCER" manifest \
      "$FEED_ROOT" \
      "$REPOSITORY_ROOT" \
      "$UIKIT_APP" \
      "$OXIDE_APP" \
      "$CONTROLLER_RUNNER" \
      "$CONTROLLER_XCTEST" \
      "$BUILD_PROVENANCE" \
      "$candidate" \
      && cmp -s "$RAW_ROOT/evidence-manifest.json" "$candidate"
}

run_controller_test()
{
   local scheme="$1"
   local result_bundle="$2"
   local log="$3"
   local execution_allowance="$4"
   xcrun xcodebuild \
      -project "$PROJECT" \
      -scheme "$scheme" \
      -configuration Release \
      -destination "id=$XCODE_DEVICE_ID" \
      -derivedDataPath "$DERIVED_DATA" \
      -resultBundlePath "$result_bundle" \
      -allowProvisioningUpdates \
      -test-timeouts-enabled YES \
      -maximum-test-execution-time-allowance "$execution_allowance" \
      DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM_ID" \
      CODE_SIGN_STYLE=Automatic \
      CODE_SIGN_IDENTITY="Apple Development" \
      test-without-building 2>&1 | tee "$log"
   return "${PIPESTATUS[0]}"
}

copy_documents()
{
   local bundle_id="$1"
   local destination="$2"
   local output="$3"
   xcrun devicectl device copy from --device "$CORE_DEVICE_ID" \
      --domain-type appDataContainer --domain-identifier "$bundle_id" \
      --source Documents --destination "$destination" --quiet \
      --json-output "$output"
}

move_result_bundle()
{
   local result_bundle="$1"
   local name="$2"
   local discarded="$BUILD_ROOT/$name.xcresult"
   if [[ ! -d "$result_bundle" ]] || ! measure_tree "$result_bundle"
   then
      return 1
   fi
   if [[ "$TREE_BYTES" -gt "$MAX_RESULT_BUNDLE_BYTES" ]] \
      || [[ "$RESULT_BUNDLE_BYTES" -gt $((MAX_RESULT_BUNDLE_BYTES - TREE_BYTES)) ]]
   then
      echo "XCTest result bundles exceeded their predeclared $MAX_RESULT_BUNDLE_BYTES-byte fuse" >&2
      return 1
   fi
   RESULT_BUNDLE_BYTES=$((RESULT_BUNDLE_BYTES + TREE_BYTES))
   mv -- "$result_bundle" "$discarded" \
      && [[ ! -e "$result_bundle" && -d "$discarded" ]]
}

merge_app_documents()
{
   local source="$1"
   local destination="$2"
   local label="$3"
   local path
   local relative
   local target
   local invalid
   local list="$BUILD_ROOT/merge-$label.list"
   if [[ ! -d "$source" ]]
   then
      echo "$label primary Documents copy is missing" >&2
      return 1
   fi
   invalid="$(find "$source" -mindepth 1 ! -type f ! -type d -print -quit)" || return 1
   if [[ -n "$invalid" ]]
   then
      echo "$label primary Documents contain a symlink or nonregular entry: $invalid" >&2
      return 1
   fi
   find "$source" -type f -print0 >"$list" || return 1
   while IFS= read -r -d '' path
   do
      relative="${path#"$source"/}"
      target="$destination/$relative"
      if [[ -e "$target" ]] && ! cmp -s "$path" "$target"
      then
         echo "$label primary evidence conflicts with retained smoke evidence at $relative" >&2
         return 1
      fi
   done <"$list"
   while IFS= read -r -d '' path
   do
      relative="${path#"$source"/}"
      target="$destination/$relative"
      if [[ ! -e "$target" ]]
      then
         mkdir -p "$(dirname "$target")" || return 1
         cp -p "$path" "$target" || return 1
      fi
   done <"$list"
}

if ! remove_device_processes preclean uikit FeedV1UIKit \
   || ! remove_device_processes preclean oxide FeedV1Oxide \
   || ! remove_controller_processes preclean \
   || ! remove_existing_app com.oxide.feed-v1.uikit uikit preclean \
   || ! remove_existing_app com.oxide.feed-v1.oxide oxide preclean \
   || ! remove_existing_app "$CONTROLLER_BUNDLE_ID" controller preclean
then
   exit 1
fi

xcrun devicectl device install app --device "$CORE_DEVICE_ID" "$UIKIT_APP" --json-output "$RAW_ROOT/install-uikit.json" --quiet
if [[ $? -ne 0 ]]
then
   echo "UIKit app install failed" >&2
   exit 1
fi
xcrun devicectl device install app --device "$CORE_DEVICE_ID" "$OXIDE_APP" --json-output "$RAW_ROOT/install-oxide.json" --quiet
if [[ $? -ne 0 ]]
then
   echo "Oxide app install failed" >&2
   exit 1
fi

if ! require_unlocked_device "$RAW_ROOT/lock-before-test.json"
then
   exit 1
fi

if ! check_directory_bytes "$BUILD_ROOT" "external build root before launch" "$MAX_BUILD_ROOT_BYTES" \
   || ! check_retained_evidence_fuses
then
   exit 1
fi
PRELAUNCH_FUSES_ADMITTED=true
TEST_STARTED="$(date +%s)"
if [[ "$MODE" == "full" ]]
then
   SMOKE_RESULT_BUNDLE="$RAW_ROOT/feed-v1-smoke.xcresult"
   run_controller_test FeedV1PilotSmoke "$SMOKE_RESULT_BUNDLE" "$RAW_ROOT/test-smoke.log" 1200
   TEST_STATUS=$?
   if [[ $TEST_STATUS -eq 0 ]]
   then
      SMOKE_TEST_SUCCEEDED=true
   else
      record_failure smoke-test "smoke XCTest failed"
   fi
   if [[ -d "$SMOKE_RESULT_BUNDLE" ]] \
      && xcrun xcresulttool export attachments --path "$SMOKE_RESULT_BUNDLE" --output-path "$ATTACHMENT_ROOT" \
         >"$RAW_ROOT/attachment-export.log" 2>&1 \
      && "$REDUCER" verify-attachments "$ATTACHMENT_ROOT" \
         >>"$RAW_ROOT/attachment-export.log" 2>&1
   then
      VERIFIED_ATTACHMENT_COUNT=$EXPECTED_ATTACHMENTS
   else
      echo "smoke result-bundle attachment export or verification failed" >&2
      record_failure smoke-attachments "smoke result-bundle attachment verification failed"
      TEST_STATUS=1
   fi
   SMOKE_RESULT_REMOVED=false
   if move_result_bundle "$SMOKE_RESULT_BUNDLE" feed-v1-smoke
   then
      SMOKE_RESULT_REMOVED=true
   else
      echo "smoke result bundle could not be removed from retained evidence" >&2
      record_failure smoke-result-cleanup "the smoke result bundle remained in retained evidence"
      TEST_STATUS=1
   fi
   SMOKE_EXPORT_OK=true
   if ! copy_documents com.oxide.feed-v1.uikit "$RAW_ROOT/uikit-documents" "$RAW_ROOT/copy-smoke-uikit.json"
   then
      SMOKE_EXPORT_OK=false
   fi
   if ! copy_documents com.oxide.feed-v1.oxide "$RAW_ROOT/oxide-documents" "$RAW_ROOT/copy-smoke-oxide.json"
   then
      SMOKE_EXPORT_OK=false
   fi
   if ! copy_documents "$CONTROLLER_BUNDLE_ID" "$RAW_ROOT/controller-documents" "$RAW_ROOT/copy-smoke-controller.json"
   then
      SMOKE_EXPORT_OK=false
   fi
   if [[ -e "$RAW_ROOT/device-after-smoke.json" ]] \
      || ! xcrun devicectl device info details --device "$CORE_DEVICE_ID" \
         --json-output "$RAW_ROOT/device-after-smoke.json" --quiet
   then
      SMOKE_EXPORT_OK=false
   fi
   if [[ "$SMOKE_EXPORT_OK" != "true" ]]
   then
      echo "smoke evidence export failed" >&2
      record_failure smoke-export "the smoke app, controller, or device endpoint evidence could not be exported"
      TEST_STATUS=1
   fi
   if [[ $TEST_STATUS -eq 0 ]]
   then
      if ! verify_evidence_manifest_snapshot before-primary
      then
         echo "build or source identity changed before primary admission" >&2
         record_failure pre-primary-identity "the manifest-bound source or binaries changed after smoke"
         TEST_STATUS=1
      elif ! "$REDUCER" admit-smoke-prefix "$RESULT_ROOT" >"$RAW_ROOT/smoke-admission.log" 2>&1
      then
         echo "smoke admission blocked; primary population will not start" >&2
         record_failure smoke-admission "the frozen smoke reducer gate blocked"
         TEST_STATUS=1
      else
         SMOKE_ADMITTED=true
      fi
   fi

   PRIMARY_RESULT_REMOVED=false
   if [[ $TEST_STATUS -eq 0 ]]
   then
      if ! require_unlocked_device "$RAW_ROOT/lock-before-primary.json"
      then
         record_failure primary-lock "the device was locked before the primary population"
         TEST_STATUS=1
      else
         PRIMARY_RESULT_BUNDLE="$RAW_ROOT/feed-v1-primary.xcresult"
         PRIMARY_ALLOWANCE=$((1200 - ($(date +%s) - TEST_STARTED)))
         if [[ $PRIMARY_ALLOWANCE -le 0 ]]
         then
            echo "global 20-minute fuse expired before primary" >&2
            record_failure primary-runtime-cap "the global 20-minute fuse expired before primary"
            TEST_STATUS=1
         else
            PRIMARY_STARTED=true
            run_controller_test FeedV1Pilot "$PRIMARY_RESULT_BUNDLE" "$RAW_ROOT/test-primary.log" "$PRIMARY_ALLOWANCE"
            TEST_STATUS=$?
            if [[ $TEST_STATUS -eq 0 ]]
            then
               PRIMARY_TEST_SUCCEEDED=true
            else
               record_failure primary-test "primary XCTest failed or exceeded the remaining global runtime"
            fi
         fi
         PRIMARY_ATTACHMENT_ROOT="$BUILD_ROOT/primary-attachments"
         if [[ -d "$PRIMARY_RESULT_BUNDLE" ]] \
            && mkdir -p "$PRIMARY_ATTACHMENT_ROOT" \
            && xcrun xcresulttool export attachments --path "$PRIMARY_RESULT_BUNDLE" \
               --output-path "$PRIMARY_ATTACHMENT_ROOT" >"$RAW_ROOT/primary-attachment-export.log" 2>&1 \
            && "$REDUCER" verify-no-attachments "$PRIMARY_ATTACHMENT_ROOT" \
               >>"$RAW_ROOT/primary-attachment-export.log" 2>&1
         then
            :
         else
            echo "primary result bundle did not prove an attachment-free population" >&2
            record_failure primary-attachments "the primary result bundle did not prove zero attachments"
            TEST_STATUS=1
         fi
         if move_result_bundle "$PRIMARY_RESULT_BUNDLE" feed-v1-primary
         then
            PRIMARY_RESULT_REMOVED=true
         else
            echo "primary result bundle could not be removed from retained evidence" >&2
            record_failure primary-result-cleanup "the primary result bundle remained in retained evidence"
            TEST_STATUS=1
         fi

         PRIMARY_DOCUMENTS="$BUILD_ROOT/primary-documents"
         PRIMARY_EXPORT_OK=true
         mkdir -p "$PRIMARY_DOCUMENTS" || PRIMARY_EXPORT_OK=false
         if ! copy_documents com.oxide.feed-v1.uikit "$PRIMARY_DOCUMENTS/uikit" "$RAW_ROOT/copy-primary-uikit.json"
         then
            PRIMARY_EXPORT_OK=false
         fi
         if ! copy_documents com.oxide.feed-v1.oxide "$PRIMARY_DOCUMENTS/oxide" "$RAW_ROOT/copy-primary-oxide.json"
         then
            PRIMARY_EXPORT_OK=false
         fi
         if ! copy_documents "$CONTROLLER_BUNDLE_ID" "$PRIMARY_DOCUMENTS/controller" "$RAW_ROOT/copy-primary-controller.json"
         then
            PRIMARY_EXPORT_OK=false
         fi
         if ! merge_app_documents "$PRIMARY_DOCUMENTS/uikit" "$RAW_ROOT/uikit-documents" UIKit \
            || ! merge_app_documents "$PRIMARY_DOCUMENTS/oxide" "$RAW_ROOT/oxide-documents" Oxide \
            || ! merge_app_documents "$PRIMARY_DOCUMENTS/controller" "$RAW_ROOT/controller-documents" controller
         then
            PRIMARY_EXPORT_OK=false
         fi
         if [[ "$PRIMARY_EXPORT_OK" != "true" ]]
         then
            echo "primary evidence export or smoke/primary merge failed" >&2
            record_failure primary-export "primary app or controller evidence could not be merged without conflict"
            TEST_STATUS=1
         fi
         if [[ ! -f "$RAW_ROOT/controller-documents/oxide-feed-v1-controller-runtime-primary.json" ]]
         then
            echo "primary controller runtime proof is missing" >&2
            record_failure primary-runtime "the primary controller runtime proof is missing"
            TEST_STATUS=1
         fi
      fi
   fi
   if [[ "$SMOKE_RESULT_REMOVED" == "true" && "$PRIMARY_RESULT_REMOVED" == "true" ]]
   then
      RESULT_BUNDLE_REMOVED=true
   fi
   if [[ "$SMOKE_TEST_SUCCEEDED" == "true" && "$PRIMARY_TEST_SUCCEEDED" == "true" ]]
   then
      TEST_SUCCEEDED=true
   fi
else
   RESULT_BUNDLE="$RAW_ROOT/feed-v1-$MODE.xcresult"
   run_controller_test "$SCHEME" "$RESULT_BUNDLE" "$RAW_ROOT/test.log" 1200
   TEST_STATUS=$?
   if [[ $TEST_STATUS -eq 0 ]]
   then
      TEST_SUCCEEDED=true
   else
      record_failure "$MODE-test" "$MODE XCTest failed"
   fi
   if [[ -d "$RESULT_BUNDLE" ]] \
      && xcrun xcresulttool export attachments --path "$RESULT_BUNDLE" --output-path "$ATTACHMENT_ROOT" \
         >"$RAW_ROOT/attachment-export.log" 2>&1 \
      && "$REDUCER" verify-attachments "$ATTACHMENT_ROOT" \
         >>"$RAW_ROOT/attachment-export.log" 2>&1
   then
      VERIFIED_ATTACHMENT_COUNT=$EXPECTED_ATTACHMENTS
   else
      echo "result-bundle attachment export or verification failed" >&2
      record_failure "$MODE-attachments" "$MODE result-bundle attachment verification failed"
      TEST_STATUS=1
   fi
   if move_result_bundle "$RESULT_BUNDLE" "feed-v1-$MODE"
   then
      RESULT_BUNDLE_REMOVED=true
   else
      echo "result bundle could not be removed from retained evidence" >&2
      record_failure "$MODE-result-cleanup" "$MODE result bundle remained in retained evidence"
      TEST_STATUS=1
   fi
   if ! copy_documents com.oxide.feed-v1.uikit "$RAW_ROOT/uikit-documents" "$RAW_ROOT/copy-uikit.json" \
      || ! copy_documents com.oxide.feed-v1.oxide "$RAW_ROOT/oxide-documents" "$RAW_ROOT/copy-oxide.json" \
      || ! copy_documents "$CONTROLLER_BUNDLE_ID" "$RAW_ROOT/controller-documents" "$RAW_ROOT/copy-controller.json"
   then
      echo "$MODE evidence export failed" >&2
      record_failure "$MODE-export" "$MODE app or controller evidence export failed"
      TEST_STATUS=1
   fi
fi
TEST_ENDED="$(date +%s)"
if ! verify_evidence_manifest_snapshot after-test
then
   echo "build or source identity changed during device testing" >&2
   record_failure post-test-identity "the manifest-bound source or binaries changed during device testing"
   TEST_STATUS=1
fi
if [[ -e "$RAW_ROOT/device-after.json" ]] \
   || ! xcrun devicectl device info details --device "$CORE_DEVICE_ID" \
      --json-output "$RAW_ROOT/device-after.json" --quiet
then
   echo "final physical-device endpoint capture failed or was not exclusive" >&2
   record_failure final-device-endpoint "the final physical-device endpoint was missing, stale, or could not be captured"
   TEST_STATUS=1
fi

if verify_source_snapshot "device test and evidence export"
then
   SOURCE_SNAPSHOT_PRESERVED=true
else
   record_failure source-snapshot "the Git source snapshot changed during device testing"
   TEST_STATUS=1
fi

if remove_existing_app com.oxide.feed-v1.uikit uikit postclean \
   && remove_existing_app com.oxide.feed-v1.oxide oxide postclean
then
   APPS_UNINSTALLED=true
else
   record_failure app-cleanup "one or both measured apps could not be uninstalled"
   TEST_STATUS=1
fi
if remove_device_processes postclean uikit FeedV1UIKit
then
   UIKIT_PROCESS_ABSENT=true
else
   record_failure uikit-process-cleanup "the UIKit process survived cleanup"
   TEST_STATUS=1
fi
if remove_device_processes postclean oxide FeedV1Oxide
then
   OXIDE_PROCESS_ABSENT=true
else
   record_failure oxide-process-cleanup "the Oxide process survived cleanup"
   TEST_STATUS=1
fi
if remove_controller_processes postclean
then
   CONTROLLER_PROCESS_ABSENT=true
else
   record_failure controller-process-cleanup "the controller process survived cleanup"
   TEST_STATUS=1
fi
if remove_existing_app "$CONTROLLER_BUNDLE_ID" controller postclean
then
   CONTROLLER_UNINSTALLED=true
else
   record_failure controller-cleanup "the controller runner could not be uninstalled"
   TEST_STATUS=1
fi

if measure_tree "$BUILD_ROOT"
then
   BUILD_ROOT_BYTES="$TREE_BYTES"
fi
if [[ "$BUILD_ROOT_BYTES" -eq 0 || "$BUILD_ROOT_BYTES" -gt "$MAX_BUILD_ROOT_BYTES" ]]
then
   echo "external build root exceeded its predeclared $MAX_BUILD_ROOT_BYTES-byte fuse" >&2
   record_failure build-cap "the external build root exceeded its predeclared byte fuse"
   TEST_STATUS=1
fi
if [[ "$BUILD_ROOT" == /tmp/oxide-feed-v1-build.* && -d "$BUILD_ROOT" ]]
then
   /bin/rm -rf -- "$BUILD_ROOT"
fi
if [[ ! -e "$BUILD_ROOT" ]]
then
   BUILD_REMOVED=true
else
   record_failure build-cleanup "the external build root survived cleanup"
   TEST_STATUS=1
fi

RUNTIME_SECONDS=$((TEST_ENDED - TEST_STARTED))
if [[ "$RUNTIME_SECONDS" -gt 1200 ]]
then
   echo "official physical-device runtime exceeded 20 minutes" >&2
   record_failure runtime-cap "the measured device workflow exceeded 20 minutes"
   TEST_STATUS=1
fi
if [[ "$SOURCE_SNAPSHOT_PRESERVED" != "true" ]] \
   || ! verify_source_snapshot "final reduction"
then
   SOURCE_SNAPSHOT_PRESERVED=false
   record_failure final-source-snapshot "the source snapshot was not preserved through final reduction"
   TEST_STATUS=1
fi
if ! check_retained_evidence_fuses
then
   record_failure retained-evidence-cap "retained evidence exceeded a predeclared resource fuse"
   TEST_STATUS=1
fi
if [[ "$MODE" == "full" ]]
then
   SMOKE_ADMITTED_JSON=$SMOKE_ADMITTED
   RUNNER_PHASES_JSON="[
    { \"phase\": \"smoke\", \"started\": true, \"succeeded\": $SMOKE_TEST_SUCCEEDED },
    { \"phase\": \"primary\", \"started\": $PRIMARY_STARTED, \"succeeded\": $PRIMARY_TEST_SUCCEEDED }
  ]"
else
   SMOKE_ADMITTED_JSON=null
   RUNNER_PHASES_JSON="[
    { \"phase\": \"smoke\", \"started\": true, \"succeeded\": $TEST_SUCCEEDED }
  ]"
fi
if [[ $TEST_STATUS -ne 0 && -z "$FIRST_FAILURE_STAGE" ]]
then
   record_failure runner "an unclassified runner stage failed"
fi
if [[ $TEST_STATUS -eq 0 ]]
then
   RUNNER_STATUS=complete
   FAILURE_STAGE_JSON=null
   FAILURE_REASON_JSON=null
else
   RUNNER_STATUS=blocked
   FAILURE_STAGE_JSON="\"$FIRST_FAILURE_STAGE\""
   FAILURE_REASON_JSON="\"$FIRST_FAILURE_REASON\""
fi
cat >"$RAW_ROOT/runner.json" <<EOF
{
  "schema": "oxide.feed-v1.runner",
  "schema_revision": 1,
  "mode": "$MODE",
  "status": "$RUNNER_STATUS",
  "first_failure_stage": $FAILURE_STAGE_JSON,
  "first_failure_reason": $FAILURE_REASON_JSON,
  "smoke_admitted": $SMOKE_ADMITTED_JSON,
  "phases": $RUNNER_PHASES_JSON
}
EOF
RAW_EVIDENCE_BYTES="$(du -sk "$RAW_ROOT" | awk '{print $1 * 1024}')"
cat >"$RAW_ROOT/cleanup.json" <<EOF
{
  "schema": "oxide.feed-v1.cleanup",
  "schema_revision": 3,
  "test_succeeded": $TEST_SUCCEEDED,
  "verified_attachment_count": $VERIFIED_ATTACHMENT_COUNT,
  "apps_uninstalled": $APPS_UNINSTALLED,
  "controller_uninstalled": $CONTROLLER_UNINSTALLED,
  "uikit_process_absent": $UIKIT_PROCESS_ABSENT,
  "oxide_process_absent": $OXIDE_PROCESS_ABSENT,
  "controller_process_absent": $CONTROLLER_PROCESS_ABSENT,
  "prelaunch_fuses_admitted": $PRELAUNCH_FUSES_ADMITTED,
  "resource_limits": {
    "external_build_bytes": $MAX_BUILD_ROOT_BYTES,
    "result_bundle_bytes": $MAX_RESULT_BUNDLE_BYTES,
    "retained_evidence_bytes": $MAX_RETAINED_EVIDENCE_BYTES,
    "retained_file_count": $MAX_RETAINED_FILE_COUNT,
    "retained_file_bytes": $MAX_RETAINED_FILE_BYTES
  },
  "source_snapshot_preserved": $SOURCE_SNAPSHOT_PRESERVED,
  "external_build_removed": $BUILD_REMOVED,
  "result_bundle_removed": $RESULT_BUNDLE_REMOVED,
  "reducer_binary_absent_from_result_root": true,
  "external_build_bytes": $BUILD_ROOT_BYTES,
  "result_bundle_bytes": $RESULT_BUNDLE_BYTES,
  "raw_evidence_bytes": $RAW_EVIDENCE_BYTES,
  "retained_file_count": $RETAINED_FILE_COUNT,
  "largest_retained_file_bytes": $LARGEST_RETAINED_FILE_BYTES,
  "runtime_seconds": $RUNTIME_SECONDS
}
EOF

if [[ "$MODE" == "smoke" ]]
then
   "$REDUCER" verify-smoke "$RESULT_ROOT"
   REDUCE_STATUS=$?
else
   "$REDUCER" reduce "$RESULT_ROOT" "$RESULT_ROOT/latest.json" "$RESULT_ROOT/latest.md"
   REDUCE_STATUS=$?
   if [[ $REDUCE_STATUS -eq 0 ]]
   then
      if ! REPORT_STATUS="$(/usr/bin/plutil -extract status raw -o - "$RESULT_ROOT/latest.json")" \
         || [[ "$REPORT_STATUS" != "complete" ]]
      then
         echo "reducer report is not complete" >&2
         REDUCE_STATUS=1
      fi
   fi
fi
if [[ "$REDUCER_RUN_ROOT" == /tmp/oxide-feed-v1-reducer.* && -d "$REDUCER_RUN_ROOT" ]]
then
   /bin/rm -rf -- "$REDUCER_RUN_ROOT"
fi
if [[ ! -e "$REDUCER_RUN_ROOT" ]]
then
   REDUCER_REMOVED=true
else
   echo "external reducer executable was not removed" >&2
   REDUCE_STATUS=1
fi
if ! check_retained_evidence_fuses
then
   exit 1
fi

END_SECONDS="$(date +%s)"
echo "feed-v1 $MODE: test_status=$TEST_STATUS reducer_status=$REDUCE_STATUS reducer_removed=$REDUCER_REMOVED wall_seconds=$((END_SECONDS - START_SECONDS)) result_root=$RESULT_ROOT"
if [[ $TEST_STATUS -ne 0 || $REDUCE_STATUS -ne 0 ]]
then
   exit 1
fi
