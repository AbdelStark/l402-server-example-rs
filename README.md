# L402 Server Example (Rust)

A Rust implementation of an API paywalled with the L402 protocol. This project demonstrates how to implement a server that charges per API call using Lightning Network and Coinbase Commerce payments.

## Features

- User creation and management
- Credits-based API paywall system
- Stock ticker data API endpoint
- Payment processing via:
  - Lightning Network
  - Coinbase Commerce
- Webhook handling for payment confirmations
- Redis-based data storage

## API Endpoints

- **GET /signup** - Create a new user account with 1 free credit
- **GET /info** - Get current user info (requires authentication)
- **GET /ticker/{symbol}** - Get stock data for a symbol, costs 1 credit (requires authentication)
- **GET /block** - Get the latest Bitcoin block hash, costs 1 credit (requires authentication)
- **POST /l402/payment-request** - Initiate a payment to purchase more credits
- **POST /webhook/lightning** - Lightning payment webhooks
- **POST /webhook/coinbase** - Coinbase payment webhooks

## Authentication

Authentication is handled via a simple token-based system. When a user signs up, they receive a unique ID that serves as their API token. This token must be included in the `Authorization` header as a Bearer token:

```
Authorization: Bearer <user_id>
```

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [Redis](https://redis.io/download) (for data storage)
- Optional: LND node or other Lightning payment provider
- Optional: Coinbase Commerce account

### Configuration

Configuration is done via environment variables or a `.env` file:

```
# Server configuration
PORT=8080
HOST=127.0.0.1

# Redis configuration
REDIS_URL=redis://localhost:6379

# Lightning payment configuration
LIGHTNING_ENABLED=true
# LND_REST_ENDPOINT=https://localhost:8080
# LND_MACAROON_HEX=your_macaroon_hex_here
# LND_CERT_PATH=/path/to/tls.cert

# Coinbase payment configuration
COINBASE_ENABLED=true
# COINBASE_API_KEY=your_coinbase_api_key
# COINBASE_WEBHOOK_SECRET=your_webhook_secret

# Credit offers
OFFERS_JSON=[{"id":"offer1","title":"1 Credit Package","description":"Purchase 1 credit for API access","credits":1,"amount":0.01,"currency":"USD"},{"id":"offer2","title":"5 Credits Package","description":"Purchase 5 credits for API access","credits":5,"amount":0.05,"currency":"USD"}]

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

### Getting Stock Data

```bash
curl -H "Authorization: Bearer 57d102ff-7188-4eff-b868-2d46d649aafe" http://localhost:8080/ticker/AAPL
```

If the user has credits, they will receive the stock data:
```json
{
  "additional_data": {
    "current_price": 232.87,
    "eps": 6.07,
    "pe_ratio": 38.36
  },
  "financial_data": [
    {
      "fiscal_date_ending": "2023-09-30",
      "total_revenue": 391035000000.0,
      "net_income": 93736000000.0
    },
    ...
  ]
}
```

If the user is out of credits, they will receive a 402 Payment Required response with available offers.

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

## Development

To run the code with hot-reloading for development:

```bash
cargo install cargo-watch
cargo watch -x run
```

Run linting:

```bash
cargo clippy
```

Format code:

```bash
cargo fmt
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.