# Coral Redis - Command Line Interface

Coral Redis provides a comprehensive command line interface for configuring and running the Redis-compatible server.

## Quick Start

```bash
# Build the project
cargo build --release

# Start with default settings (memory backend, localhost:6379)
./target/release/coral-redis

# Or with cargo run
cargo run
```

## Command Line Options

### Server Configuration

```bash
# Custom host and port
./target/release/coral-redis --host 0.0.0.0 --port 8080

# Bind to all interfaces
./target/release/coral-redis --host 0.0.0.0
```

### Storage Backends

```bash
# Memory backend (default)
./target/release/coral-redis --storage memory

# LMDB backend (requires --features lmdb-backend)
cargo build --release --features lmdb-backend
./target/release/coral-redis --storage lmdb --lmdb-path ./data.lmdb

# S3 backend (requires --features s3-backend)
cargo build --release --features s3-backend
./target/release/coral-redis --storage s3 --s3-bucket my-bucket --s3-prefix redis/
```

### Logging Options

```bash
# Verbose logging (INFO level)
./target/release/coral-redis --verbose

# Debug logging (DEBUG level)
./target/release/coral-redis --debug

# Quiet mode (WARN level, default)
./target/release/coral-redis
```

### Configuration File

```bash
# Load configuration from JSON file
./target/release/coral-redis --config config.json

# CLI arguments override file settings
./target/release/coral-redis --config config.json --port 8080 --verbose
```

Example `config.json`:

```json
{
  "server": {
    "host": "127.0.0.1",
    "port": 6379
  },
  "storage": {
    "backend": "memory"
  }
}
```

### Help and Version

```bash
# Show help
./target/release/coral-redis --help

# Show version
./target/release/coral-redis --version
```

## Configuration Precedence

Settings are applied in the following order (later overrides earlier):

1. **Default values** (memory backend, localhost:6379)
2. **Environment variables** (legacy support)
3. **Configuration file** (if specified with `--config`)
4. **Command line arguments** (highest priority)

## Environment Variables (Legacy Support)

For backward compatibility, these environment variables are still supported:

```bash
export STORAGE_BACKEND=memory
export REDIS_HOST=127.0.0.1
export REDIS_PORT=6379
export LMDB_PATH=./data.lmdb
export S3_BUCKET=my-bucket
export S3_PREFIX=redis/
export AWS_REGION=us-west-2
```

## Backend-Specific Options

### LMDB Backend

```bash
# Required: path to LMDB database file
--lmdb-path ./redis-data.lmdb

# The directory will be created if it doesn't exist
--lmdb-path /var/lib/coral-redis/data.lmdb
```

### S3 Backend

```bash
# Required: S3 bucket name
--s3-bucket my-redis-bucket

# Optional: key prefix for namespacing
--s3-prefix production/redis/

# Optional: AWS region (uses default AWS config if not specified)
--aws-region us-west-2
```

AWS credentials are loaded from the standard AWS credential chain:

- Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
- AWS credentials file (`~/.aws/credentials`)
- IAM instance profile (for EC2 instances)

## Examples

```bash
# Development setup
./target/release/coral-redis --verbose

# Production setup with LMDB persistence
./target/release/coral-redis \
  --host 0.0.0.0 \
  --port 6379 \
  --storage lmdb \
  --lmdb-path /var/lib/redis/data.lmdb

# Cloud setup with S3 backend
./target/release/coral-redis \
  --host 0.0.0.0 \
  --storage s3 \
  --s3-bucket prod-redis-cluster \
  --s3-prefix cache/ \
  --aws-region us-east-1 \
  --verbose

# Load balancer health check endpoint
./target/release/coral-redis --port 6380 --verbose

# Development with config file
./target/release/coral-redis --config dev-config.json --debug
```

## Testing the Server

After starting the server, test with any Redis client:

```bash
# Redis CLI
redis-cli ping
redis-cli set mykey "Hello World"
redis-cli get mykey

# Custom port
redis-cli -p 8080 ping

# Python
import redis
r = redis.Redis(host='localhost', port=6379)
r.ping()
```
