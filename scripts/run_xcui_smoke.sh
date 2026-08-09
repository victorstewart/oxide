#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT="${ROOT_DIR}/oxide/host/ios-app/App/OxideHost.xcodeproj"
SCHEME="OxideHost"
DEFAULT_DEST="platform=iOS Simulator,name=iPhone 16"
DEVELOPMENT_TEAM_ID="${OXIDE_IOS_DEVELOPMENT_TEAM:-${DEVELOPMENT_TEAM:-}}"
if [[ -n "${XCUI_DESTINATION:-}" ]]
then
   DESTINATION="${XCUI_DESTINATION}"
else
   USING_PHYSICAL=0
   DESTINATION=""
   if command -v xcrun >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1
   then
      PHYSICAL_DEST="$(python3 <<'PY'
import json
import subprocess
import sys

try:
    raw = subprocess.check_output(
        ["xcrun", "xcdevice", "list"], text=True, timeout=10
    )
except Exception:
    sys.exit(0)

try:
    devices = json.loads(raw)
except json.JSONDecodeError:
    sys.exit(0)

def runtime_key(device):
    # Prefer wired devices first, then by OS version descending
    interface = device.get("interface") or ""
    wired_rank = 0 if interface.lower() == "usb" else 1
    version = device.get("operatingSystemVersion") or ""
    numbers = []
    for part in version.replace("(", " ").replace(")", " ").replace(".", " ").split():
        try:
            numbers.append(int(part))
        except ValueError:
            continue
    while len(numbers) < 3:
        numbers.append(0)
    return (wired_rank, -numbers[0], -numbers[1], -numbers[2])

viable = [
    dev for dev in devices
    if not dev.get("simulator", True) and dev.get("available", False)
       and (dev.get("platform") or "").endswith("iphoneos")
]
if not viable:
    sys.exit(0)

chosen = sorted(viable, key=runtime_key)[0]
identifier = chosen.get("identifier")
if identifier:
    print(f"id={identifier}")
PY
)"
      if [[ -n "${PHYSICAL_DEST}" ]]
      then
         DESTINATION="${PHYSICAL_DEST}"
         USING_PHYSICAL=1
      fi
   fi
   if [[ -z "${DESTINATION}" ]] && command -v xcrun >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1
   then
      DEVICE_ID="$(python3 <<'PY'
import json
import subprocess
import sys

try:
    raw = subprocess.check_output([
        "xcrun",
        "simctl",
        "list",
        "devices",
        "--json"
    ], text=True)
except Exception:
    sys.exit(0)

try:
    data = json.loads(raw)
except json.JSONDecodeError:
    sys.exit(0)

def runtime_key(key):
    prefix = "com.apple.CoreSimulator.SimRuntime.iOS-"
    if not key.startswith(prefix):
        return (0, 0, 0)
    parts = key[len(prefix):].split('-')
    numbers = []
    for part in parts:
        try:
            numbers.append(int(part))
        except ValueError:
            numbers.append(0)
    while len(numbers) < 3:
        numbers.append(0)
    return tuple(numbers[:3])

for runtime in sorted(data.get("devices", {}), key=runtime_key, reverse=True):
    for device in data["devices"].get(runtime, []):
        if device.get("isAvailable") and device.get("name", "").startswith("iPhone"):
            print(device.get("udid", ""))
            sys.exit(0)
PY
)"
      if [[ -n "${DEVICE_ID}" ]]
      then
         DESTINATION="platform=iOS Simulator,id=${DEVICE_ID}"
      fi
   fi
   if [[ -z "${DESTINATION}" ]]
   then
      DESTINATION="${DEFAULT_DEST}"
   fi
fi
RESULT_BUNDLE="${ROOT_DIR}/artifacts/ui/ResultBundle"
DERIVED_DATA="${ROOT_DIR}/artifacts/ui/DerivedData"

rm -rf "${RESULT_BUNDLE}" "${RESULT_BUNDLE}.xcresult"
mkdir -p "$(dirname "${RESULT_BUNDLE}")"

if ! command -v xcodebuild >/dev/null 2>&1
then
   echo "xcodebuild not found; skipping XCUI smoke" >&2
   exit 0
fi

set +e
XCB_ARGS=(
   -project "${PROJECT}"
   -scheme "${SCHEME}"
   -destination "${DESTINATION}"
   -resultBundlePath "${RESULT_BUNDLE}"
   -derivedDataPath "${DERIVED_DATA}"
   -parallel-testing-enabled NO
   -only-testing:OxideHostUITests/OxideHostUITests/testWindowLaunchSmoke
   test
)
if [[ "${USING_PHYSICAL:-0}" -eq 1 ]]
then
   XCB_ARGS+=(-allowProvisioningUpdates -allowProvisioningDeviceRegistration)
   if [[ -n "${DEVELOPMENT_TEAM_ID}" ]]
   then
      XCB_ARGS+=("DEVELOPMENT_TEAM=${DEVELOPMENT_TEAM_ID}")
   fi
fi
xcodebuild "${XCB_ARGS[@]}"
status=$?
set -e

if [[ ${status} -ne 0 ]]
then
   echo "XCUI launch smoke failed" >&2
   exit ${status}
fi

exit 0
