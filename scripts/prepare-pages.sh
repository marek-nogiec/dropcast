#!/bin/sh

set -eu

output_dir="${1:-target/pages}"
fallback_url="https://github.com/marek-nogiec/dropcast/releases"
download_url="${DROPCAST_APP_DOWNLOAD_URL:-$fallback_url}"

case "$output_dir" in
    "" | "/" | ".")
        echo "refusing unsafe Pages output directory: $output_dir" >&2
        exit 1
        ;;
esac

mkdir -p "$output_dir"
cp -R docs/. "$output_dir/"

python3 - "$output_dir/index.html" "$fallback_url" "$download_url" <<'PY'
from pathlib import Path
import html
import re
import sys

index_path = Path(sys.argv[1])
fallback_url = sys.argv[2]
download_url = html.escape(sys.argv[3], quote=True)
source = index_path.read_text(encoding="utf-8")
pattern = re.compile(
    rf'href="{re.escape(fallback_url)}"(?P<spacing>\s+)data-app-download'
)
replacement = rf'href="{download_url}"\g<spacing>data-app-download'
rendered, count = pattern.subn(replacement, source)

if count != 3:
    raise SystemExit(f"expected 3 app download links, found {count}")

index_path.write_text(rendered, encoding="utf-8")
PY
