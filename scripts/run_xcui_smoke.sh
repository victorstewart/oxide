#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT="${ROOT_DIR}/oxide/host/ios-app/App/OxideHost.xcodeproj"
SCHEME="OxideHost"
DEFAULT_DEST="platform=iOS Simulator,name=iPhone 16"
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
EXPORT_DIR="${OXIDE_UI_EXPORT:-${ROOT_DIR}/artifacts/ui}"
RESULT_BUNDLE="${ROOT_DIR}/artifacts/ui/ResultBundle"
DERIVED_DATA="${ROOT_DIR}/artifacts/ui/DerivedData"

rm -rf "${EXPORT_DIR}" "${RESULT_BUNDLE}" "${DERIVED_DATA}"
mkdir -p "${EXPORT_DIR}"

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
   OXIDE_UI_EXPORT="${EXPORT_DIR}"
   test
)
if [[ "${USING_PHYSICAL:-0}" -eq 1 ]]
then
   XCB_ARGS+=(-allowProvisioningUpdates -allowProvisioningDeviceRegistration)
fi
xcodebuild "${XCB_ARGS[@]}"
status=$?
set -e

if [[ ${status} -ne 0 ]]
then
   echo "XCUI tests failed" >&2
   exit ${status}
fi

shopt -s nullglob
pngs=("${EXPORT_DIR}"/*.png)
if [[ ${#pngs[@]} -eq 0 ]]
then
   echo "No exported screenshots found in ${EXPORT_DIR}; skipping golden comparison (UISwitch automation disabled in headless run)" >&2
   exit 0
fi

declare -A GOLDENS
GOLDENS["controls-scene"]="${ROOT_DIR}/goldens/static/scene_controls/default/default/baseline.png"
GOLDENS["collection-scene"]="${ROOT_DIR}/goldens/static/scene_collection/default/default/baseline.png"
GOLDENS["zoom-scene"]="${ROOT_DIR}/goldens/static/scene_zoom/default/default/baseline.png"
GOLDENS["nine-slice-scene"]="${ROOT_DIR}/goldens/static/nine_slice/default/default/baseline.png"
GOLDENS["sdf-scene"]="${ROOT_DIR}/goldens/static/scene_text/default/default/baseline.png"
GOLDENS["animations-scene"]="${ROOT_DIR}/goldens/static/style_effects/default/default/baseline.png"

missing=0
failed=0
for png_path in "${pngs[@]}"
do
   name="$(basename "${png_path}" .png)"
   golden="${GOLDENS[${name}]:-}"
   if [[ -z "${golden}" ]]
   then
      echo "[skip] no golden mapping for ${name}" >&2
      ((missing+=1))
      continue
   fi
   if [[ ! -f "${golden}" ]]
   then
      echo "[fail] golden not found for ${name}: ${golden}" >&2
      ((failed+=1))
      continue
   fi
   if cmp -s "${png_path}" "${golden}"
   then
      echo "[ok] ${name} matches golden"
   else
      echo "[diff] ${name} diverges from ${golden}" >&2
      ((failed+=1))
   fi
 done

if [[ ${failed} -gt 0 ]]
then
   exit 1
fi

if [[ ${missing} -gt 0 ]]
then
   echo "completed with ${missing} screenshots without golden coverage" >&2
fi

exit 0
