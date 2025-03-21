# Scripts

This directory contains various scripts to help with development, testing, and deployment of the L402 server.

## CI Scripts (`/scripts/ci/`)

Scripts related to continuous integration, testing, and release.

- **`ci-check.sh`**: Runs all CI checks locally (formatting, linting, tests, security audit)
- **`check-redis.sh`**: Verifies Redis is available (used by CI checks)
- **`prepare-release.sh`**: Prepares a new release by updating version numbers and creating a tag

## Demo Scripts (`/scripts/demo/`)

Scripts to help demonstrate and test the L402 payment flow.

- **`setup-local.sh`**: Sets up a local development environment (starts Redis, creates .env file)
- **`test-flow.sh`**: Tests the full L402 payment flow from signup to payment required 
- **`simulate-payment.sh`**: Simulates a successful Lightning payment (without requiring an actual payment)

## Usage

### CI Scripts

```bash
# Run all CI checks
./scripts/ci/ci-check.sh

# Prepare a new release
./scripts/ci/prepare-release.sh v0.1.0
```

### Demo Scripts

```bash
# Set up local environment (start Redis, create .env)
./scripts/demo/setup-local.sh

# Test the full L402 flow (signup, use credits, get 402 and payment options)
./scripts/demo/test-flow.sh

# Test with custom host and port
./scripts/demo/test-flow.sh -h api.example.com -p 3000

# Skip payment step (just show info)
./scripts/demo/test-flow.sh -s

# Simulate a successful payment after getting payment hash
./scripts/demo/simulate-payment.sh -u "user_id_here" -i "payment_hash_here"
```