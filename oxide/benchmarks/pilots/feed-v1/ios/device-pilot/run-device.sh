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
CONTROLLER_BUNDLE_ID="com.oxide.feed-v1.controller.xctrunner"
SCHEME="FeedV1PilotSmoke"
EXPECTED_ATTACHMENTS=6
if [[ "$MODE" == "full" ]]
then
   SCHEME="FeedV1Pilot"
fi

if ! mkdir -p "$RAW_ROOT" "$ATTACHMENT_ROOT"
then
   echo "could not create the external result root" >&2
   exit 1
fi
APPS_UNINSTALLED=false
CONTROLLER_UNINSTALLED=false
CONTROLLER_PROCESS_ABSENT=false
SOURCE_SNAPSHOT_PRESERVED=false
BUILD_REMOVED=false
RESULT_BUNDLE_REMOVED=false
REDUCER_REMOVED=false
TEST_SUCCEEDED=false
VERIFIED_ATTACHMENT_COUNT=0
TEST_STATUS=1
START_SECONDS="$(date +%s)"

cleanup_backstop()
{
   if [[ -d "$BUILD_ROOT" ]]
   then
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

remove_controller_processes()
{
   local stage="$1"
   local before="$RAW_ROOT/$stage-controller-process-before.json"
   local after="$RAW_ROOT/$stage-controller-process-after.json"
   local index=0
   local pid
   local processes
   if ! xcrun devicectl device info processes --device "$CORE_DEVICE_ID" \
      --filter "executable.absoluteString ENDSWITH '/FeedV1Controller-Runner'" \
      --json-output "$before" --quiet
   then
      echo "could not inspect the controller process during $stage" >&2
      return 1
   fi
   while pid="$(/usr/bin/plutil -extract "result.runningProcesses.$index.processIdentifier" raw -o - "$before" 2>/dev/null)"
   do
      if [[ ! "$pid" =~ ^[[:digit:]]+$ ]] \
         || ! xcrun devicectl device process terminate --device "$CORE_DEVICE_ID" --pid "$pid" --quiet
      then
         echo "could not terminate controller process $pid during $stage" >&2
         return 1
      fi
      index=$((index + 1))
   done
   if ! xcrun devicectl device info processes --device "$CORE_DEVICE_ID" \
      --filter "executable.absoluteString ENDSWITH '/FeedV1Controller-Runner'" \
      --json-output "$after" --quiet \
      || ! processes="$(/usr/bin/plutil -extract result.runningProcesses json -o - "$after")" \
      || [[ "$processes" != "[]" ]]
   then
      echo "controller process survived cleanup during $stage" >&2
      return 1
   fi
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

trap cleanup_backstop EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

export FEED_V1_SOURCE_ROOT="$SCRIPT_DIR"
if ! mkdir -p "$GENERATED_PROJECT_ROOT" \
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

export FEED_V1_CARGO_TARGET_DIR="$RUST_TARGET"
xcrun xcodebuild \
   -project "$PROJECT" \
   -scheme "$SCHEME" \
   -configuration Release \
   -destination "id=$XCODE_DEVICE_ID" \
   -derivedDataPath "$DERIVED_DATA" \
   -allowProvisioningUpdates \
   -allowProvisioningDeviceRegistration \
   DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM_ID" \
   CODE_SIGN_STYLE=Automatic \
   CODE_SIGN_IDENTITY="Apple Development" \
   build-for-testing 2>&1 | tee "$RAW_ROOT/build.log"
if [[ ${PIPESTATUS[0]} -ne 0 ]]
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
   "$RAW_ROOT/evidence-manifest.json"
if [[ $? -ne 0 ]]
then
   echo "evidence manifest failed" >&2
   exit 1
fi

if ! remove_controller_processes preclean \
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

RESULT_BUNDLE="$RAW_ROOT/feed-v1-$MODE.xcresult"
TEST_STARTED="$(date +%s)"
xcrun xcodebuild \
   -project "$PROJECT" \
   -scheme "$SCHEME" \
   -configuration Release \
   -destination "id=$XCODE_DEVICE_ID" \
   -derivedDataPath "$DERIVED_DATA" \
   -resultBundlePath "$RESULT_BUNDLE" \
   -allowProvisioningUpdates \
   DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM_ID" \
   CODE_SIGN_STYLE=Automatic \
   CODE_SIGN_IDENTITY="Apple Development" \
   test-without-building 2>&1 | tee "$RAW_ROOT/test.log"
TEST_STATUS=${PIPESTATUS[0]}
if [[ $TEST_STATUS -eq 0 ]]
then
   TEST_SUCCEEDED=true
fi
TEST_ENDED="$(date +%s)"

if [[ -d "$RESULT_BUNDLE" ]] \
   && xcrun xcresulttool export attachments --path "$RESULT_BUNDLE" --output-path "$ATTACHMENT_ROOT" \
      >"$RAW_ROOT/attachment-export.log" 2>&1 \
   && "$REDUCER" verify-attachments "$ATTACHMENT_ROOT" \
      >>"$RAW_ROOT/attachment-export.log" 2>&1
then
   VERIFIED_ATTACHMENT_COUNT=$EXPECTED_ATTACHMENTS
   DISCARDED_RESULT_BUNDLE="$BUILD_ROOT/feed-v1-$MODE.xcresult"
   if mv -- "$RESULT_BUNDLE" "$DISCARDED_RESULT_BUNDLE" \
      && [[ ! -e "$RESULT_BUNDLE" && -d "$DISCARDED_RESULT_BUNDLE" ]]
   then
      RESULT_BUNDLE_REMOVED=true
   else
      echo "verified result bundle could not be moved out of the retained root" >&2
      TEST_STATUS=1
   fi
else
   echo "result-bundle attachment export or verification failed" >&2
   TEST_STATUS=1
fi

xcrun devicectl device copy from --device "$CORE_DEVICE_ID" \
   --domain-type appDataContainer --domain-identifier com.oxide.feed-v1.uikit \
   --source Documents --destination "$RAW_ROOT/uikit-documents" --quiet \
   --json-output "$RAW_ROOT/copy-uikit.json" || true
xcrun devicectl device copy from --device "$CORE_DEVICE_ID" \
   --domain-type appDataContainer --domain-identifier com.oxide.feed-v1.oxide \
   --source Documents --destination "$RAW_ROOT/oxide-documents" --quiet \
   --json-output "$RAW_ROOT/copy-oxide.json" || true
xcrun devicectl device copy from --device "$CORE_DEVICE_ID" \
   --domain-type appDataContainer --domain-identifier "$CONTROLLER_BUNDLE_ID" \
   --source Documents --destination "$RAW_ROOT/controller-documents" --quiet \
   --json-output "$RAW_ROOT/copy-controller.json" || true
xcrun devicectl device info details --device "$CORE_DEVICE_ID" --json-output "$RAW_ROOT/device-after.json" --quiet || true

if verify_source_snapshot "device test and evidence export"
then
   SOURCE_SNAPSHOT_PRESERVED=true
else
   TEST_STATUS=1
fi

if remove_existing_app com.oxide.feed-v1.uikit uikit postclean \
   && remove_existing_app com.oxide.feed-v1.oxide oxide postclean
then
   APPS_UNINSTALLED=true
fi
if remove_controller_processes postclean
then
   CONTROLLER_PROCESS_ABSENT=true
fi
if remove_existing_app "$CONTROLLER_BUNDLE_ID" controller postclean
then
   CONTROLLER_UNINSTALLED=true
fi

BUILD_KIB="$(du -sk "$BUILD_ROOT" | awk '{print $1}')"
if [[ "$BUILD_KIB" -gt 4194304 ]]
then
   echo "external build root exceeded 4 GiB" >&2
   TEST_STATUS=1
fi
if [[ "$BUILD_ROOT" == /tmp/oxide-feed-v1-build.* && -d "$BUILD_ROOT" ]]
then
   /bin/rm -rf -- "$BUILD_ROOT"
fi
if [[ ! -e "$BUILD_ROOT" ]]
then
   BUILD_REMOVED=true
fi

RUNTIME_SECONDS=$((TEST_ENDED - TEST_STARTED))
if [[ "$RUNTIME_SECONDS" -gt 1200 ]]
then
   echo "official physical-device runtime exceeded 20 minutes" >&2
   TEST_STATUS=1
fi
if [[ "$SOURCE_SNAPSHOT_PRESERVED" != "true" ]] \
   || ! verify_source_snapshot "final reduction"
then
   SOURCE_SNAPSHOT_PRESERVED=false
   TEST_STATUS=1
fi
RAW_EVIDENCE_BYTES="$(du -sk "$RAW_ROOT" | awk '{print $1 * 1024}')"
cat >"$RAW_ROOT/cleanup.json" <<EOF
{
  "schema": "oxide.feed-v1.cleanup",
  "schema_revision": 2,
  "test_succeeded": $TEST_SUCCEEDED,
  "verified_attachment_count": $VERIFIED_ATTACHMENT_COUNT,
  "apps_uninstalled": $APPS_UNINSTALLED,
  "controller_uninstalled": $CONTROLLER_UNINSTALLED,
  "controller_process_absent": $CONTROLLER_PROCESS_ABSENT,
  "source_snapshot_preserved": $SOURCE_SNAPSHOT_PRESERVED,
  "external_build_removed": $BUILD_REMOVED,
  "result_bundle_removed": $RESULT_BUNDLE_REMOVED,
  "reducer_binary_absent_from_result_root": true,
  "raw_evidence_bytes": $RAW_EVIDENCE_BYTES,
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
FINAL_BYTES="$(du -sk "$RESULT_ROOT" | awk '{print $1 * 1024}')"
if [[ "$FINAL_BYTES" -gt 536870912 ]]
then
   echo "retained result root exceeded 512 MiB" >&2
   exit 1
fi

END_SECONDS="$(date +%s)"
echo "feed-v1 $MODE: test_status=$TEST_STATUS reducer_status=$REDUCE_STATUS reducer_removed=$REDUCER_REMOVED wall_seconds=$((END_SECONDS - START_SECONDS)) result_root=$RESULT_ROOT"
if [[ $TEST_STATUS -ne 0 || $REDUCE_STATUS -ne 0 ]]
then
   exit 1
fi
