#!/bin/bash
# Print raw / gzip -9 / xz -9 sizes (bytes) for files, and du for directories.
# Usage: sizes.sh <label>=<path> ...
set -u
printf "%-45s %12s %12s %12s\n" label raw gzip9 xz9
for kv in "$@"; do
  label=${kv%%=*}; p=${kv#*=}
  if [ -d "$p" ]; then
    raw=$(du -sk "$p" | awk '{print $1*1024}')
    tgz=$(tar -cf - -C "$(dirname "$p")" "$(basename "$p")" | gzip -9 | wc -c | tr -d ' ')
    txz=$(tar -cf - -C "$(dirname "$p")" "$(basename "$p")" | xz -9 -T0 | wc -c | tr -d ' ')
    printf "%-45s %12s %12s %12s  (dir, du)\n" "$label" "$raw" "$tgz" "$txz"
  else
    raw=$(stat -f %z "$p")
    gz=$(gzip -9 -c "$p" | wc -c | tr -d ' ')
    x=$(xz -9 -T0 -c "$p" | wc -c | tr -d ' ')
    printf "%-45s %12s %12s %12s\n" "$label" "$raw" "$gz" "$x"
  fi
done
