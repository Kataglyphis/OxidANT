#!/bin/bash
set -euo pipefail

echo "Starte GUI mit Person Detection..."
cargo run --release -p kataglyphis_cli --features="gui_unix" --bin kataglyphis_cli -- gui
