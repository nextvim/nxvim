#!/bin/sh
set -eu

if grep -R -n -E 'crossterm|crate::terminal|crate::view|crate::controller|crate::app::services|vim_ui::(Renderer|View)' src/model; then
    echo "model dependency boundary violated" >&2
    exit 1
fi

echo "architecture boundaries OK"
