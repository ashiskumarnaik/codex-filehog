# Codex FileHog

A Rust-based file monitoring tool that watches target directories for changes and stores files or entire folders of data on the Codex storage network. FileHog automatically detects new and modified files, validates them, and uploads data to Codex with configurable monitoring intervals and storage options.

Based on the specifications from [benbierens/codex-filehog](https://github.com/benbierens/codex-filehog).


## Features

- **Automatic file ingestion**: Monitors target folder for new files and automatically stores them
- **Configurable storage parameters**: Customize price, durability, proof probability, and duration
- **Load balancing**: Distribute requests across multiple Codex nodes
- **Retry logic**: Automatically retries failed operations with exponential backoff
- **Flexible output formats**: Choose between flattened or structured metadata storage
- **Purchase monitoring**: Automatically renews storage contracts before expiration
- **Cross-platform**: Supports Ubuntu, macOS, and Windows

## Prerequisites

1. One or more Codex nodes running with storage marketplace enabled
2. Codex nodes must have sufficient TST/TSTWEI tokens for storage purchases
3. Files must be between 1MB and 1GB in size
4. Target folder should contain files that won't be modified or deleted

## Quick Run

```bash
# Clone the repository
git clone https://github.com/ashiskumarnaik/codex-filehog.git
cd codex-filehog
```
```bash
# Copy and configure settings
cp config.example.toml config.toml
```
```bash
# Create directories 
mkdir -p /path/to/your/target/directory
mkdir -p /path/to/your/output/directory

# Set the directory paths in the config.toml
target_folder = "/path/to/your/target/directory"

# Output folder for metadata and logs
output_folder = "/path/to/your/output/directory"
```
```bash
# Set Codex API endpoints
codex_endpoints = [
  "http://localhost:8080",
]
```
```bash
# Run the application
RUST_LOG=trace cargo run -- -c config.toml

# The binary will be available at target/release/codex-filehog
```

## Configuration

### Command Line Arguments

```bash
# Basic usage with command line arguments
./codex-filehog --target-folder /path/to/files --output-folder /path/to/output

# Using a configuration file
./codex-filehog --config config.toml
```

### Configuration File

Create a configuration file `config.toml` based on this `config.example.toml`:

```toml
target_folder = "/path/to/your/files" # prefer providing Absolute paths
output_folder = "/path/to/output" # prefer providing Absolute paths
output_structure = "structured"  # or "flattened"

codex_endpoints = [
    "http://localhost:8080",
]

[storage_params]
price = 1000              # Price per byte per second (TSTWEI)
nodes = 10                # Number of storage nodes
tolerance = 5             # Fault tolerance
proof_probability = 100   # Proof probability (0-100)
duration_days = 6         # Storage duration (minimum 1 day)
expiry_minutes = 60       # Purchase expiry (minimum 15 minutes)
collateral = 1            # Collateral per byte (TSTWEI)
```

## Usage

### Basic Operation

1. **Prepare your files**: Place files (1MB-1GB each) in your target folder
2. **Configure the tool**: Create a config file or use command line arguments
3. **Run FileHog**: The tool will process existing files and monitor for new ones

```bash
# Using configuration file
./codex-filehog --config my-config.toml

# Using command line arguments
./codex-filehog \
  --target-folder /home/user/important-photos \
  --output-folder /home/user/filehog-output
```

### Output Formats

#### Structured Output
- Creates a `.json` file for each stored file
- Maintains the same directory structure as the target folder
- Easy to locate metadata for specific files

#### Flattened Output
- Single `files.json` file containing all file records
- Compact format with relative paths
- Easier to process programmatically

### Monitoring

FileHog continuously monitors:
- New files added to the target folder
- Storage contract status and expiration
- Failed purchases requiring retry

The tool runs until manually stopped (Ctrl+C).

## Output Metadata

Each file record contains:
- `file_path`: Original file path
- `original_cid`: CID from initial upload
- `storage_cid`: CID from storage contract
- `purchase_id`: Storage contract ID
- `created_at`: Timestamp of first processing
- `updated_at`: Timestamp of last update
- `codex_endpoint`: Codex node used
- `status`: Current status (New, Uploading, Creating, Active, Failed, Expired)
- `error`: Error message if applicable

## Error Handling

### Startup Validation
The tool validates configuration at startup and exits with clear error messages for:
- Target and output folders being the same
- Unreachable Codex endpoints
- Invalid duration/expiry values
- Missing target folder

### Runtime Errors
- **Network failures**: Retried up to 3 times with exponential backoff
- **Insufficient tokens**: Tool exits with error message
- **File upload failures**: Recorded in metadata, processing continues
- **Disk operation failures**: Tool exits with error message

### Logs and Crash Reports
- Logs written to stdout/stderr (use standard log level environment variables)
- Crash reports saved to output folder with timestamp
- Detailed error information for debugging

## Environment Variables

```bash
# Set log level (error, warn, info, debug, trace)
export RUST_LOG=info

# Run with debug logging
RUST_LOG=debug ./codex-filehog --config config.toml
```

## File Size Constraints

- **Minimum**: 1MB (Codex network requirement)
- **Maximum**: 1GB (current Codex limitation)
- Files outside this range are automatically skipped

## Storage Contract Lifecycle

1. **Upload**: File uploaded to Codex node, receives CID
2. **Purchase**: Storage contract created with specified parameters
3. **Active**: Contract started, file stored across network
4. **Monitoring**: Status checked every 5 minutes
5. **Renewal**: New contract created 1 hour before expiration

## Troubleshooting

### Common Issues

**"Target folder and output folder cannot be the same"**
- Use different paths for target and output folders

**"Endpoint X is not reachable"**
- Verify Codex node is running and accessible
- Check firewall settings and network connectivity

**"Insufficient tokens to create storage request"**
- Add more TST/TSTWEI tokens to your Codex node
- Reduce storage price or duration parameters

**"Duration must be at least 1 day"**
- Set `duration_days` to 1 or higher in configuration

### Getting Help

- Check logs for detailed error messages
- Review crash reports in output folder
- Verify Codex node status and token balance
- Ensure files are within size constraints (1MB-1GB)
