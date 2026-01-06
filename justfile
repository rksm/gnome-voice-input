default:
    @just --list

# Build the project
build:
    cargo build

build-nix:
    nix build -L .#gnome-voice-input

# Run the application
run *args="":
    RUST_LOG=debug cargo run --release -- {{ args }}

debug *args="":
    RUST_LOG=debug cargo run -- --debug {{ args }}

# Run tests
test:
    cargo test -- --test-threads=1

# Format code
fmt:
    cargo fmt

# Run lints
lint:
    cargo clippy --workspace --examples -- -D warnings

# Clean build artifacts
clean:
    cargo clean

# Install the application
install:
    cargo install --path .

# Create default config if it doesn't exist
init-config:
    mkdir -p ~/.config/gnome-voice-input
    cp config/default.toml ~/.config/gnome-voice-input/config.toml
    @echo "Config created at ~/.config/gnome-voice-input/config.toml"
    @echo "Please add your Deepgram API key to the config file"

# Check all (format, lint, test)
check: fmt lint test

# Watch for changes and rebuild
watch:
    cargo watch -x run

# Build Debian package (requires cargo-deb)
deb:
    cargo deb

console-transcriber log-level="warn":
    RUST_LOG={{ log-level }} cargo run --example simple-transcriber

# Query Deepgram API costs for the last 24 hours
deepgram-costs:
  #!/usr/bin/env bash
  set -e

  # Calculate dates for last 24 hours
  END_DATE=$(date -u +"%Y-%m-%d")
  START_DATE=$(date -u -d "yesterday" +"%Y-%m-%d")

  echo "=== Deepgram Usage Report ==="
  echo "Period: $START_DATE to $END_DATE (last 24 hours)"
  echo ""

  # Check API key scopes first
  echo "🔐 API Key Scopes:"
  SCOPES_RESPONSE=$(curl -s -X GET \
    "https://api.deepgram.com/v1/projects/$DEEPGRAM_PROJECT_ID/keys" \
    -H "Authorization: Token $DEEPGRAM_API_KEY")

  if echo "$SCOPES_RESPONSE" | jq -e '.api_keys' > /dev/null 2>&1; then
    echo "$SCOPES_RESPONSE" | jq -r '.api_keys[] | select(.api_key_id) | "  Key: \(.comment // "Unnamed")\n  Scopes: \(.scopes | join(", "))"' | head -n 4
  else
    echo "  Unable to fetch API key scopes"
    echo "  Note: You may need 'usage:read' and 'billing:read' scopes for full access"
  fi
  echo ""

  # Get current balance (may fail without billing:read scope)
  echo "📊 Current Balance:"
  BALANCE_RESPONSE=$(curl -s -X GET \
    "https://api.deepgram.com/v1/projects/$DEEPGRAM_PROJECT_ID/balances" \
    -H "Authorization: Token $DEEPGRAM_API_KEY")

  if echo "$BALANCE_RESPONSE" | jq -e '.balances' > /dev/null 2>&1; then
    echo "$BALANCE_RESPONSE" | jq -r '.balances[] | "  Balance ID: \(.balance_id)\n  Amount: $\(.amount)\n  Units: \(.units)"'
  elif echo "$BALANCE_RESPONSE" | jq -e '.category' > /dev/null 2>&1; then
    echo "  ⚠️  Insufficient permissions: billing:read scope required"
    echo "  Visit https://console.deepgram.com to check balance"
  else
    echo "  Unable to fetch balance data"
  fi

  echo ""
  echo "📈 Usage (Last 24 Hours):"

  # Get usage breakdown for last 24 hours (may fail without usage:read scope)
  USAGE_RESPONSE=$(curl -s -X GET \
    "https://api.deepgram.com/v1/projects/$DEEPGRAM_PROJECT_ID/usage?start=$START_DATE&end=$END_DATE" \
    -H "Authorization: Token $DEEPGRAM_API_KEY")

  if echo "$USAGE_RESPONSE" | jq -e '.requests' > /dev/null 2>&1; then
    echo "$USAGE_RESPONSE" | jq -r '"  Total Requests: \(.requests // 0)\n  Total Hours: \(.hours // 0)"'

    if echo "$USAGE_RESPONSE" | jq -e '.results' > /dev/null 2>&1; then
      echo "  Breakdown by model:"
      echo "$USAGE_RESPONSE" | jq -r '.results | to_entries | map("    - \(.key): \(.value.hours // 0) hours, \(.value.requests // 0) requests") | join("\n")'
    fi
  elif echo "$USAGE_RESPONSE" | jq -e '.category' > /dev/null 2>&1; then
    echo "  ⚠️  Insufficient permissions: usage:read scope required"
    echo "  Visit https://console.deepgram.com to check usage"
  else
    echo "  No usage data available"
  fi

  echo ""
  echo "💰 Cost Estimate:"
  echo "  Note: Check Deepgram pricing page for current rates"
  echo "  https://deepgram.com/pricing"
  echo ""

  # Test API connectivity
  echo "🔌 API Connectivity Test:"
  TEST_RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" -X GET \
    "https://api.deepgram.com/v1/projects/$DEEPGRAM_PROJECT_ID" \
    -H "Authorization: Token $DEEPGRAM_API_KEY")

  if [ "$TEST_RESPONSE" = "200" ]; then
    echo "  ✅ API key is valid and can access project"
  elif [ "$TEST_RESPONSE" = "401" ]; then
    echo "  ❌ Invalid API key"
  elif [ "$TEST_RESPONSE" = "403" ]; then
    echo "  ⚠️  API key valid but lacks permissions"
  else
    echo "  ⚠️  Unexpected response: HTTP $TEST_RESPONSE"
  fi
  echo ""

  echo "📝 To get full access, create an API key with these scopes:"
  echo "  - usage:read (for usage statistics)"
  echo "  - billing:read (for balance information)"
  echo "  Visit: https://console.deepgram.com"
