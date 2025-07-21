#!/bin/bash
# Post-install script for outgrep semantic search support
# This script copies ONNX Runtime libraries to the cargo bin directory

set -e

echo "Setting up semantic search support for outgrep..."

# Check if og binary exists
if [ ! -f "$HOME/.cargo/bin/og" ]; then
    echo "Error: og binary not found. Please run 'cargo install --path .' first."
    exit 1
fi

# Build target directory path
TARGET_DIR="target/release"
if [ ! -d "$TARGET_DIR" ]; then
    echo "Error: Release build directory not found. Please run 'cargo build --release' first."
    exit 1
fi

# Copy ONNX Runtime libraries
echo "Copying ONNX Runtime libraries..."
cp "$TARGET_DIR/libonnxruntime.1.16.0.dylib" "$HOME/.cargo/bin/" 2>/dev/null || true
cp "$TARGET_DIR/libonnxruntime.dylib" "$HOME/.cargo/bin/" 2>/dev/null || true

# Check if libraries were copied successfully
if [ -f "$HOME/.cargo/bin/libonnxruntime.dylib" ]; then
    echo "✅ ONNX Runtime libraries installed successfully"
    echo "✅ Semantic search is now available: og --semantic 'your query'"
else
    echo "⚠️  Warning: Could not copy ONNX Runtime libraries"
    echo "   Semantic search may not work outside the project directory"
fi

echo "Setup complete!"