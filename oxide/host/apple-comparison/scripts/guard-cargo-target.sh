#!/bin/sh

set -eu

comparison_target_dir=$1
comparison_workspace_manifest=$2
comparison_limit_kib=${OXIDE_COMPARISON_TARGET_LIMIT_KIB:-4194304}

case "$comparison_limit_kib" in
   ''|*[!0-9]*)
      echo "oxide comparison Cargo target limit must be a positive KiB integer" >&2
      exit 64
      ;;
esac

if [ "$comparison_limit_kib" -eq 0 ]
then
   echo "oxide comparison Cargo target limit must be greater than zero" >&2
   exit 64
fi

if [ ! -d "$comparison_target_dir" ]
then
   exit 0
fi

comparison_size_kib=$(/usr/bin/du -sk "$comparison_target_dir" | /usr/bin/awk '{print $1}')
if [ "$comparison_size_kib" -le "$comparison_limit_kib" ]
then
   exit 0
fi

echo "oxide comparison Cargo target exceeded ${comparison_limit_kib} KiB (${comparison_size_kib} KiB); cleaning it and failing closed" >&2
cargo clean --manifest-path "$comparison_workspace_manifest" --target-dir "$comparison_target_dir"
exit 70
