#!/usr/bin/env bash
# One-time vendoring for the openWakeWord browser runtime.
#
# Outputs (all under public/wake-word/):
#   openwakeword.js              IIFE bundle exposing window.OpenWakeWord
#   ort/ort-wasm-*.wasm          onnxruntime-web wasm runtime
#   models/melspectrogram.onnx   audio -> mel features
#   models/embedding_model.onnx  Google speech embedding (~1.3 MB)
#   models/silero_vad.onnx       voice-activity gate (~1.7 MB)
#   models/wake_word.onnx        the keyword classifier (defaults to the
#                                pretrained hey_jarvis as a fallback so
#                                something works immediately; replace with
#                                your trained "Hey Roon" .onnx when ready)
#
# Requires Node.js + npm in PATH (only used by this script — the app itself
# is pure Rust + Dioxus, no Node at runtime). Re-run any time to refresh.

set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
public_dir="$project_root/public/wake-word"
models_dir="$public_dir/models"
ort_dir="$public_dir/ort"

echo "Vendoring openWakeWord browser runtime into $public_dir"

for cmd in node npm npx; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "error: $cmd not found in PATH. Install Node.js (https://nodejs.org/) and re-run." >&2
        exit 1
    fi
done

tmp_dir="$(mktemp -d -t roon-ai-oww-XXXXXX)"
trap 'rm -rf "$tmp_dir"' EXIT

cd "$tmp_dir"

echo "  Installing openwakeword-wasm-browser + onnxruntime-web..."
npm init -y >/dev/null
npm install --silent --no-fund --no-audit \
    openwakeword-wasm-browser@^0.1.1 \
    onnxruntime-web@^1.23.2 \
    esbuild@^0.24

mkdir -p "$public_dir" "$models_dir" "$ort_dir"

echo "  Bundling to IIFE via esbuild..."
npx --no-install esbuild \
    node_modules/openwakeword-wasm-browser/src/index.js \
    --bundle \
    --format=iife \
    --global-name=OpenWakeWord \
    --platform=browser \
    --outfile="$public_dir/openwakeword.js"

echo "  Copying ONNX models..."
models_src="node_modules/openwakeword-wasm-browser/models"
for f in melspectrogram.onnx embedding_model.onnx silero_vad.onnx; do
    cp -f "$models_src/$f" "$models_dir/$f"
done

# Fallback wake-word classifier: ship pretrained "hey jarvis" so the toggle
# works the moment the build picks up the assets. Users replace this file
# with their trained "Hey Roon" .onnx (same filename) later.
fallback="$models_src/hey_jarvis_v0.1.onnx"
if [ -f "$fallback" ]; then
    cp -f "$fallback" "$models_dir/wake_word.onnx"
    echo "    seeded wake_word.onnx with hey_jarvis fallback"
else
    echo "warning: hey_jarvis_v0.1.onnx not found in npm tarball — wake_word.onnx not seeded." >&2
    echo "         The wake word will be off until you drop in your own trained classifier." >&2
fi

echo "  Copying onnxruntime-web wasm runtime..."
cp -f node_modules/onnxruntime-web/dist/ort-wasm*.wasm "$ort_dir/"

cat <<EOF

Done. Now rebuild so the assets get embedded into the single-binary server:
  dx build --release --platform web --features web
  cargo build --release --features server

Then toggle 'Hands-free wake word' on in Settings.

To upgrade from 'Hey Jarvis' (the fallback) to 'Hey Roon': train a custom
model via the official Colab — synthetic TTS data, no recordings needed:
  https://github.com/dscripka/openWakeWord#training-new-models
Save the resulting .onnx as public/wake-word/models/wake_word.onnx and rebuild.
EOF
