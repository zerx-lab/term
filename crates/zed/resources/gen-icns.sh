#!/bin/sh
# Run this on macOS to regenerate Document.icns
cd "$(dirname "$0")"
iconutil -c icns zterm.iconset -o Document.icns
echo "Document.icns generated"
