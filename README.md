# Coral Redis Server

A high-performance Redis-compatible server implementation written in Rust, featuring pluggable storage backends and comprehensive observability.

## ✨ Features

- **Redis Protocol Compatible**: Full RESP2 and RESP3 (Redis Serialization Protocol) support
- **Inline Commands**: Support for telnet-style space-separated commands
- **Protocol Auto-Detection**: Automatically detects RESP vs inline command format
- **Multiple Storage Backends**: Memory, LMDB, and S3-compatible storage
- **OpenTelemetry Metrics**: Comprehensive performance monitoring and observability
- **Async/Await**: Built on Tokio for high-performance async I/O
- **Graceful Error Handling**: Protocol errors send error responses without closing connections

## 🚀 Quick Start

### Prerequisites

- Rust 1.70+ (2021 edition)
- Cargo

### Installation

```bash
git clone https://github.com/koblas/coral.git
cd coral

# Build with default features (memory and LMDB storage)
cargo build --release

# Build with S3 support
cargo build --release --features s3-backend
```

### Running the Server

```bash
# Start with in-memory storage (default)
./target/release/coral-redis
# or explicitly:
./target/release/coral-redis --storage memory

# Start with LMDB persistence
./target/release/coral-redis --storage lmdb --lmdb-path ./data.lmdb

# Start with S3 backend (requires s3-backend feature)
cargo build --release --features s3-backend
./target/release/coral-redis --storage s3 --s3-bucket my-bucket --s3-region us-west-2

# Enable verbose logging
./target/release/coral-redis --verbose

# Custom host and port
./target/release/coral-redis --host 0.0.0.0 --port 6380
```

## 📋 Supported Redis Commands

| Command      | Description                   | Status |
| ------------ | ----------------------------- | ------ |
| `PING`       | Test server connectivity      | ✅     |
| `SET`        | Set key-value pair            | ✅     |
| `GET`        | Retrieve value by key         | ✅     |
| `DEL`        | Delete keys                   | ✅     |
| `EXISTS`     | Check key existence           | ✅     |
| `DBSIZE`     | Get database size             | ✅     |
| `FLUSHDB`    | Clear database                | ✅     |
| `COMMAND`    | Get command info              | ✅     |
| `HELLO`      | Protocol negotiation (RESP3)  | ✅     |
| `SET ... EX` | Set with expiration           | ✅     |
| `CONFIG GET` | Get configuration parameters  | ✅     |

### Protocol Support

#### RESP2 (Default)
Standard Redis protocol with 5 data types:
- Simple Strings (`+`)
- Errors (`-`)
- Integers (`:`)
- Bulk Strings (`$`)
- Arrays (`*`)

#### RESP3
Enhanced protocol with additional types:
- Null (`_`)
- Boolean (`#`)
- Double (`,`)
- Set (`~`)
- Map (`%`)

Use the `HELLO` command to negotiate protocol version:
```bash
# Switch to RESP3
HELLO 3

# Stay on RESP2 (default)
HELLO 2
```

#### Inline Commands
Supports telnet-style commands for easy testing:
```bash
telnet localhost 6379
> PING
+PONG
> SET mykey myvalue
+OK
> GET mykey
$7
myvalue
> CONFIG GET port
*2
$4
port
$4
6379
```

### Configuration Management

The `CONFIG GET` command allows querying server configuration parameters:

```bash
# Get single parameter
CONFIG GET port

# Get multiple parameters
CONFIG GET port bind

# Get all parameters with wildcard
CONFIG GET *
```

**Supported Parameters:**
- `port` - Server port number
- `bind` / `host` - Server bind address
- `storage-backend` - Active storage backend (memory/lmdb/s3)
- `maxmemory` - Maximum memory (0 = unlimited)
- `maxmemory-policy` - Eviction policy (noeviction)
- `save` - Persistence snapshot settings
- `appendonly` - AOF persistence status (no)
- `databases` - Number of databases (1)

## ⚙️ Configuration

### Command Line Options

```bash
coral-redis [OPTIONS]

Options:
  -p, --port <PORT>              Server port [default: 6379]
  -h, --host <HOST>              Server host [default: 127.0.0.1]
  -s, --storage <STORAGE>        Storage backend [default: memory] [possible values: memory, lmdb, s3]
      --lmdb-path <PATH>         LMDB database path [default: ./coral.db]
      --lmdb-map-size <SIZE>     LMDB map size in MB [default: 1024]
      --s3-bucket <BUCKET>       S3 bucket name
      --s3-region <REGION>       S3 region [default: us-east-1]
      --s3-endpoint <ENDPOINT>   Custom S3 endpoint URL
  -v, --verbose                  Enable verbose logging
  -d, --debug                    Enable debug logging
      --help                     Print help
      --version                  Print version
```

### Environment Variables

- `CORAL_PORT` - Server port
- `CORAL_HOST` - Server host
- `CORAL_STORAGE` - Storage backend
- `CORAL_LMDB_PATH` - LMDB path
- `AWS_ACCESS_KEY_ID` - S3 access key
- `AWS_SECRET_ACCESS_KEY` - S3 secret key

## 🏗️ Storage Backends

### Memory Storage

- **Use Case**: Development, testing, caching
- **Features**: Fastest performance, volatile storage
- **Configuration**: No additional setup required

### LMDB Storage

- **Use Case**: Single-node persistence, high read performance
- **Features**: ACID transactions, memory-mapped files
- **Configuration**: Always available (no feature flag required)

```bash
./target/release/coral-redis --storage lmdb --lmdb-path ./data.lmdb
```

### S3 Storage

- **Use Case**: Distributed storage, backup, archival
- **Features**: Scalable, durable, multi-region support
- **Configuration**: Requires `s3-backend` feature flag and AWS credentials

```bash
cargo build --release --features s3-backend
export AWS_ACCESS_KEY_ID=your_access_key
export AWS_SECRET_ACCESS_KEY=your_secret_key
./target/release/coral-redis --storage s3 --s3-bucket my-bucket
```

## 📊 Observability & Metrics

Coral includes comprehensive OpenTelemetry metrics for production monitoring:

### Available Metrics

- **Server Metrics**:

  - `coral_connections_total` - Total client connections
  - `coral_requests_total` - Total requests processed
  - `coral_request_duration_seconds` - Request latency
  - `coral_errors_total` - Error counts by type

- **Command Metrics**:

  - `coral_commands_total` - Commands executed by type
  - `coral_command_duration_seconds` - Command execution time

- **Storage Metrics**:
  - `coral_storage_operations_total` - Storage operations by backend
  - `coral_storage_operation_duration_seconds` - Storage latency
  - `coral_keys_total` - Total keys stored

### Integration

Metrics are exported via OpenTelemetry and can be collected by:

- Prometheus + Grafana
- Jaeger
- DataDog
- Any OTLP-compatible collector

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run with coverage
cargo test --coverage

# Run integration tests only
cargo test --test integration_tests

# Run benchmarks
cargo bench
```

### Test Coverage

- **Unit Tests**: 69 tests covering RESP2/RESP3 protocol, inline commands, storage backends, and command handlers
- **Integration Tests**: 5 end-to-end tests including protocol error recovery and CONFIG command
- **Coverage**: Comprehensive testing of core functionality, RESP3 types, CONFIG command, and error handling

## 🏛️ Architecture

```
┌─────────────────┐    ┌──────────────────────────┐    ┌─────────────────┐
│   TCP Client    │────│  Protocol Detection      │────│   Handler       │
│  (RESP/Telnet)  │    │  (RESP2/RESP3/Inline)    │    │  (versioned)    │
└─────────────────┘    └──────────────────────────┘    └─────────────────┘
                                                                  │
                       ┌──────────────────┐                      │
                       │   Metrics        │──────────────────────┤
                       └──────────────────┘                      │
                                                                  │
                       ┌──────────────────┐                      │
                       │ Storage Backends │──────────────────────┘
                       │ ┌─────────────┐  │
                       │ │   Memory    │  │
                       │ ├─────────────┤  │
                       │ │   LMDB      │  │
                       │ ├─────────────┤  │
                       │ │     S3      │  │
                       │ └─────────────┘  │
                       └──────────────────┘
```

### Key Components

- **Protocol Layer** (`src/protocol/`): RESP2/RESP3 parsing, inline command support, auto-detection
- **Server Layer** (`src/server/`): Connection handling, command processing, protocol version management
- **Storage Layer** (`src/storage/`): Pluggable storage backend abstraction
- **Metrics Layer** (`src/metrics/`): OpenTelemetry instrumentation
- **Telemetry** (`src/telemetry/`): Observability configuration

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass (`cargo test`)
6. Commit your changes (`git commit -m 'Add amazing feature'`)
7. Push to the branch (`git push origin feature/amazing-feature`)
8. Open a Pull Request

### Development Setup

```bash
# Clone with all features
git clone https://github.com/koblas/coral.git
cd coral

# Install development dependencies
cargo install cargo-watch cargo-tarpaulin

# Run tests continuously
cargo watch -x test

# Check code coverage
cargo tarpaulin --out html
```

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Links

- [Redis Protocol Specification](https://redis.io/docs/reference/protocol-spec/)
- [OpenTelemetry Rust](https://docs.rs/opentelemetry/)
- [Tokio Documentation](https://tokio.rs/)

## 🙏 Acknowledgments

- Redis team for the protocol specification
- Tokio team for the async runtime
- OpenTelemetry community for observability standards
