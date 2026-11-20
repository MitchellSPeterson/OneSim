#!/usr/bin/env bash
# Both guest targets already require a JDK. Skip the generator when no resources exist.
emit_android_resources() {
  local source=$1 package=$2 output=$3 root folder
  root=$source
  if [ ! -d "$root" ]; then root="$(dirname "$root")"; fi
  for folder in res src/main/res app/src/main/res; do
    if [ -d "$root/$folder" ]; then
      java "$(dirname "${BASH_SOURCE[0]}")/AndroidResources.java" "$source" "$package" "$output"
      return
    fi
  done
}
