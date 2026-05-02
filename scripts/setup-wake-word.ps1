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
# Requires Node.js + npm in PATH (only used by this script -- the app itself
# is pure Rust + Dioxus, no Node at runtime). Re-run any time to refresh.

$ErrorActionPreference = 'Stop'

$projectRoot = (Resolve-Path "$PSScriptRoot/..").Path
$publicDir   = Join-Path $projectRoot 'public/wake-word'
$modelsDir   = Join-Path $publicDir   'models'
$ortDir      = Join-Path $publicDir   'ort'

Write-Host "Vendoring openWakeWord browser runtime into $publicDir" -ForegroundColor Cyan

# 1. Sanity check Node + npm are reachable.
foreach ($cmd in @('node', 'npm', 'npx')) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        throw "$cmd not found in PATH. Install Node.js (https://nodejs.org/) and re-run."
    }
}

# 2. Working dir for the install.
$tmpDir = Join-Path $env:TEMP "roon-ai-oww-$(Get-Random)"
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null
try {
    Push-Location $tmpDir

    # 3. npm install both packages locally (no global pollution).
    Write-Host '  Installing openwakeword-wasm-browser + onnxruntime-web...' -ForegroundColor Gray
    & npm init -y | Out-Null
    & npm install --silent --no-fund --no-audit `
        openwakeword-wasm-browser@^0.1.1 `
        onnxruntime-web@^1.23.2 `
        esbuild@^0.24
    if ($LASTEXITCODE -ne 0) { throw "npm install failed" }

    # 4. Bundle to IIFE.
    New-Item -ItemType Directory -Force -Path $publicDir, $modelsDir, $ortDir | Out-Null
    Write-Host '  Bundling to IIFE via esbuild...' -ForegroundColor Gray
    & npx --no-install esbuild `
        'node_modules/openwakeword-wasm-browser/src/index.js' `
        --bundle `
        --format=iife `
        --global-name=OpenWakeWord `
        --platform=browser `
        --outfile="$publicDir/openwakeword.js"
    if ($LASTEXITCODE -ne 0) { throw "esbuild failed" }

    # 5. Copy ONNX models.
    Write-Host '  Copying ONNX models...' -ForegroundColor Gray
    $modelsSrc = 'node_modules/openwakeword-wasm-browser/models'
    foreach ($file in @('melspectrogram.onnx', 'embedding_model.onnx', 'silero_vad.onnx')) {
        Copy-Item -Force (Join-Path $modelsSrc $file) (Join-Path $modelsDir $file)
    }

    # Fallback wake-word classifier: ship the pretrained "hey jarvis" so the
    # toggle works the moment the build picks up the assets. Users replace
    # this file with their trained "Hey Roon" .onnx (same filename) later.
    $fallback = Join-Path $modelsSrc 'hey_jarvis_v0.1.onnx'
    if (Test-Path $fallback) {
        Copy-Item -Force $fallback (Join-Path $modelsDir 'wake_word.onnx')
        Write-Host '    seeded wake_word.onnx with hey_jarvis fallback' -ForegroundColor DarkGray
    } else {
        Write-Warning "Could not find hey_jarvis_v0.1.onnx in the npm tarball -- wake_word.onnx not seeded. The wake word will be off until you train and drop in your own classifier."
    }

    # 6. Copy onnxruntime-web runtime. Each wasm variant has a paired .mjs
    # ESM loader stub — ORT 1.23+ loads the .mjs first to bootstrap, so we
    # need both extensions. We ship all variants (base / asyncify / jsep /
    # jspi) because ORT picks one at runtime based on browser feature
    # detection (Chromium tries JSEP first for WebGPU compatibility).
    # Total ~80 MB; trim manually after install if binary size is critical.
    Write-Host '  Copying onnxruntime-web runtime (.wasm + .mjs)...' -ForegroundColor Gray
    Get-ChildItem 'node_modules/onnxruntime-web/dist/' -Filter 'ort-wasm*' |
        Where-Object { $_.Extension -in '.wasm', '.mjs' } |
        Copy-Item -Destination $ortDir -Force
} finally {
    Pop-Location
    Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
}

Write-Host "`nDone. Now rebuild so the assets get embedded into the single-binary server:" -ForegroundColor Green
Write-Host "  dx build --release --platform web --features web" -ForegroundColor Yellow
Write-Host "  cargo build --release --features server" -ForegroundColor Yellow
Write-Host "`nThen toggle 'Hands-free wake word' on in Settings." -ForegroundColor Green
Write-Host "`nTo upgrade from 'Hey Jarvis' (the fallback) to 'Hey Roon': train a custom" -ForegroundColor Cyan
Write-Host "model via the official Colab -- synthetic TTS data, no recordings needed:" -ForegroundColor Cyan
Write-Host "  https://github.com/dscripka/openWakeWord#training-new-models" -ForegroundColor Yellow
Write-Host "Save the resulting .onnx as public/wake-word/models/wake_word.onnx and rebuild." -ForegroundColor Cyan
