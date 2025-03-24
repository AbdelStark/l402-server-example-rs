<div align="center">

<a href="https://github.com/AbdelStark/l402-server-example-rs/actions/workflows/ci.yml"><img alt="GitHub Workflow Status" src="https://img.shields.io/github/actions/workflow/status/AbdelStark/l402-server-example-rs/ci.yml?style=for-the-badge" height=30></a>
<a href="https://bitcoin.org/"> <img alt="Bitcoin" src="https://img.shields.io/badge/Bitcoin-000?style=for-the-badge&logo=bitcoin&logoColor=white" height=30></a>

</div>

# L402 Server Example in Rust

A Rust implementation of an API paywalled with the [L402 protocol](https://www.l402.org/). This project demonstrates how to implement a server that charges per API call using Lightning Network or other payment methods to get access to paywalled resources.

[Live Demo web app](https://l402.starknetonbitcoin.com/) - [Demo frontend repo](https://github.com/AbdelStark/l402-shield)

## Features

- User creation and management
- Credits-based API paywall system
- Bitcoin blockchain data API endpoint
- Payment processing via:
  - Lightning Network
- Webhook handling for payment confirmations
- Redis-based data storage and caching

## API Endpoints

- **GET /signup** - Create a new user account with 1 free credit
- **GET /info** - Get current user info (requires authentication)
- **GET /block** - Get the latest Bitcoin block hash, costs 1 credit (requires authentication)
- **POST /l402/payment-request** - Initiate a payment to purchase more credits
- **POST /credits-payment-options** - Get available credit purchase options
- **POST /webhook/lightning** - Lightning payment webhooks
- **POST /webhook/coinbase** - Coinbase payment webhooks

## Authentication

Authentication is handled via a simple token-based system. When a user signs up, they receive a unique ID that serves as their API token. This token must be included in the `Authorization` header as a Bearer token:

```text
Authorization: Bearer <user_id>
```

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [Redis](https://redis.io/download) (for data storage)
- Optional: LNBits account (for Lightning Network payments)
- Optional: Coinbase Commerce account

### Configuration

Configuration is done via environment variables or a `.env` file:

```text
# Server configuration
PORT=8080
HOST=127.0.0.1

# Redis configuration
REDIS_URL=redis://localhost:6379

# Payment request URL (optional, defaults to http://HOST:PORT/l402/payment-request)
# PAYMENT_REQUEST_URL=https://your-domain.com/l402/payment-request

# Lightning payment configuration
LIGHTNING_ENABLED=true
# LNBits configuration (preferred)
# LNBITS_URL=https://legend.lnbits.com
# LNBITS_ADMIN_KEY=your_admin_key_here
# LNBITS_INVOICE_READ_KEY=your_invoice_read_key_here
# LNBITS_WEBHOOK_KEY=your_webhook_verification_key_here

# Coinbase payment configuration
COINBASE_ENABLED=true
# COINBASE_API_KEY=your_coinbase_api_key
# COINBASE_WEBHOOK_SECRET=your_webhook_secret

# Credit offers
OFFERS_JSON='{"id":"offer1","title":"1 Credit Package","description":"Purchase 1 credit for API access","credits":1,"amount":0.01,"currency":"USD"},{"id":"offer2","title":"5 Credits Package","description":"Purchase 5 credits for API access","credits":5,"amount":0.05,"currency":"USD"}]'

# Logging configuration
RUST_LOG=info,l402_server_example_rs=debug
```

A `.env.example` file is provided as a template.

### Running with Docker

The easiest way to run the server is with Docker and Docker Compose:

```bash
# Build and start the containers
docker-compose up -d

# View logs
docker-compose logs -f
```

This will start the server and a Redis instance.

### Running Locally

To run the server locally:

1. Start a Redis server:

   ```bash
   redis-server
   ```

2. Build and run the application:

   ```bash
   cargo build
   cargo run
   ```

## Example Usage

### Creating a User

```bash
curl http://localhost:8080/signup
```

Response:

```json
{
  "id": "57d102ff-7188-4eff-b868-2d46d649aafe",
  "credits": 1,
  "created_at": "2024-03-20T02:39:44Z",
  "last_credit_update_at": "2024-03-20T02:39:44Z"
}
```

### Getting User Info

```bash
curl -H "Authorization: Bearer 57d102ff-7188-4eff-b868-2d46d649aafe" http://localhost:8080/info
```

Response:

```json
{
  "id": "57d102ff-7188-4eff-b868-2d46d649aafe",
  "credits": 1,
  "created_at": "2024-03-20T02:39:44Z",
  "last_credit_update_at": "2024-03-20T02:39:44Z"
}
```

### Getting Bitcoin Block Data

```bash
curl -H "Authorization: Bearer 57d102ff-7188-4eff-b868-2d46d649aafe" http://localhost:8080/block
```

If the user has credits, they will receive the latest Bitcoin block hash:

```json
{
  "hash": "000000000000000000007b05bde2eb0be32cc10ec811cb636728e647e7cc0c63",
  "timestamp": "2024-03-20T04:15:32Z"
}
```

If the user is out of credits, they will receive a 402 Payment Required response with available offers:

```json
{
  "expiry": "2024-03-20T04:45:32Z",
  "offers": [
    {
      "id": "offer1",
      "title": "1 Credit Package",
      "description": "Purchase 1 credit for API access",
      "credits": 1,
      "amount": 0.01,
      "currency": "USD"
    },
    {
      "id": "offer2",
      "title": "5 Credits Package",
      "description": "Purchase 5 credits for API access",
      "credits": 5,
      "amount": 0.05,
      "currency": "USD"
    }
  ],
  "payment_context_token": "57d102ff-7188-4eff-b868-2d46d649aafe",
  "payment_request_url": "http://localhost:8080/l402/payment-request"
}
```

### Initiating a Payment

```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"offer_id":"offer1","payment_method":"lightning","payment_context_token":"57d102ff-7188-4eff-b868-2d46d649aafe"}' \
  http://localhost:8080/l402/payment-request
```

Response for Lightning:

```json
{
  "lightning_invoice": "lnbc...",
  "offer_id": "offer1",
  "expires_at": "2024-03-20T03:09:44Z"
}
```

Response for Coinbase:

```json
{
  "checkout_url": "https://commerce.coinbase.com/charges/...",
  "address": "0x...",
  "asset": "USDC",
  "chain": "base-mainnet",
  "offer_id": "offer1",
  "expires_at": "2024-03-20T03:09:44Z"
}
```

## Understanding the L402 Payment Flow

This project demonstrates the L402 payment protocol flow:

1. A user signs up and receives 1 free credit
2. The user makes a request to `/block` to get Bitcoin block data
3. After using their credit, the next request returns a 402 Payment Required response
4. The response includes available payment options (offers)
5. The user selects an offer and initiates a payment via Lightning or other payment methods
6. Once payment is confirmed (via webhook), credits are added to the user's account
7. The user can now make another request to `/block` using their new credits

This implementation shows how micropayments can be used to monetize API access with Bitcoin, making it suitable for applications where users pay small amounts for specific pieces of data.

## Development

To run the code with hot-reloading for development:

```bash
cargo install cargo-watch
cargo watch -x run
```

Run all checks and tests (requires Redis running):

```bash
./scripts/ci-check.sh
```

Run linting:

```bash
cargo clippy
```

Format code:

```bash
cargo fmt
```

Prepare a new release:

```bash
./scripts/prepare-release.sh v1.0.0
```

## CI/CD Workflows

This project uses GitHub Actions for continuous integration and deployment:

### CI Workflow

On every push and pull request to the main branch, the CI workflow runs:

1. **Code Checks**:

   - Code formatting verification
   - Clippy linting
   - Compilation check

2. **Tests**:

   - Runs all unit and integration tests
   - Generates code coverage metrics

3. **Security Audit**:

   - Scans dependencies for known vulnerabilities with cargo-audit

4. **Build**:

   - Builds the release binary
   - Uploads the binary as an artifact

5. **Docker**:
   - Builds the Docker image (on main branch only)

### Release Workflow

When a new release is published or triggered manually:

1. **Release Build**:

   - Builds the release binary
   - Creates a distributable archive with documentation
   - Uploads the archive as a release asset

2. **Docker Publish**:
   - Builds and pushes the Docker image to GitHub Container Registry
   - Tags the image with the release version and latest

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

Built with 🧡 by [@AbdelStark](https://github.com/AbdelStark)

Feel free to follow me on Nostr if you'd like, using my public key:

```text
npub1hr6v96g0phtxwys4x0tm3khawuuykz6s28uzwtj5j0zc7lunu99snw2e29
```

Or just **scan this QR code** to find me:

![Nostr Public Key QR Code](https://hackmd.io/_uploads/SkAvwlYYC.png)
