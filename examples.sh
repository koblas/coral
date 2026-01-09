#!/bin/bash

# Coral Redis CLI Examples
# This script demonstrates various ways to start the Coral Redis server

echo "=== Coral Redis CLI Examples ==="
echo

# Build the project first
echo "Building coral-redis..."
cargo build --release
echo

# Example 1: Basic memory backend (default)
echo "1. Starting with memory backend (default):"
echo "   ./target/release/coral-redis"
echo "   ./target/release/coral-redis --storage memory"
echo

# Example 2: Custom host and port
echo "2. Custom host and port:"
echo "   ./target/release/coral-redis --host 0.0.0.0 --port 8080"
echo

# Example 3: With verbose logging
echo "3. Enable verbose logging:"
echo "   ./target/release/coral-redis --verbose"
echo

# Example 4: With debug logging
echo "4. Enable debug logging:"
echo "   ./target/release/coral-redis --debug"
echo

# Example 5: Using config file
echo "5. Load from config file:"
echo "   ./target/release/coral-redis --config config.json"
echo

# Example 6: CLI args override config file
echo "6. CLI args override config file:"
echo "   ./target/release/coral-redis --config config.json --port 8080 --verbose"
echo

# Example 7: LMDB backend (requires feature)
echo "7. LMDB backend (build with --features lmdb-backend):"
echo "   cargo build --release --features lmdb-backend"
echo "   ./target/release/coral-redis --storage lmdb --lmdb-path ./data.lmdb"
echo

# Example 8: S3 backend (requires feature and AWS config)
echo "8. S3 backend (build with --features s3-backend):"
echo "   cargo build --release --features s3-backend"
echo "   export AWS_REGION=us-west-2"
echo "   ./target/release/coral-redis --storage s3 --s3-bucket my-redis-bucket --s3-prefix redis/"
echo

# Example 9: Environment variables (legacy support)
echo "9. Environment variables (legacy):"
echo "   export STORAGE_BACKEND=memory"
echo "   export REDIS_HOST=0.0.0.0"  
echo "   export REDIS_PORT=6380"
echo "   ./target/release/coral-redis"
echo

# Example 10: Help and version
echo "10. Help and version info:"
echo "   ./target/release/coral-redis --help"
echo "   ./target/release/coral-redis --version"
echo

echo "=== Test Commands ==="
echo "After starting the server, test with:"
echo "  redis-cli ping"
echo "  redis-cli set mykey 'Hello World'"
echo "  redis-cli get mykey"
echo "  redis-cli del mykey"
echo